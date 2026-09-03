use crate::JournalConfig;
use crate::JournalError;
use crate::SegmentInfo;
use crate::ThreadRevisionRange;
use crate::recovery::FrameReadCursor;
use crate::recovery::read_pointer;
use crate::recovery::visit_segment_pointers;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadHead;
use praxis_thread_store_contracts::ThreadId;

pub(crate) fn fold_range<S>(
    config: &JournalConfig,
    thread_id: ThreadId,
    head: ThreadHead,
    segments: &[SegmentInfo],
    range: ThreadRevisionRange,
    mut state: S,
    mut fold: impl FnMut(&mut S, &ThreadEventEnvelope),
) -> Result<S, JournalError> {
    visit_range(config, thread_id, head, segments, range, |event| {
        fold(&mut state, &event)
    })?;
    Ok(state)
}

fn visit_range(
    config: &JournalConfig,
    thread_id: ThreadId,
    head: ThreadHead,
    segments: &[SegmentInfo],
    range: ThreadRevisionRange,
    mut visit: impl FnMut(ThreadEventEnvelope),
) -> Result<(), JournalError> {
    if range.end_inclusive > head.revision {
        return Err(JournalError::InvalidRange(format!(
            "requested end {:?} exceeds thread head {:?}",
            range.end_inclusive, head.revision
        )));
    }
    let mut observed = 0_u64;
    let mut cursor = FrameReadCursor::default();
    for segment in segment_window(segments, range) {
        cursor.reset_position();
        visit_segment_pointers(segment, |pointer, _, file| {
            cursor.reset_position();
            let frame = read_pointer(
                &pointer,
                file,
                &mut cursor,
                &segment.path,
                thread_id,
                config.max_frame_payload_bytes,
            )?;
            for event in frame.stored.into_events().into_iter().filter(|event| {
                event.revision >= range.start && event.revision <= range.end_inclusive
            }) {
                let expected_revision = range.start.checked_advance(observed).ok_or_else(|| {
                    JournalError::InvalidRange("revision range overflow".to_string())
                })?;
                if event.revision != expected_revision {
                    return Err(JournalError::InvalidRange(
                        "journal does not contain the complete requested range".to_string(),
                    ));
                }
                observed = observed.checked_add(1).ok_or_else(|| {
                    JournalError::InvalidRange("revision range overflow".to_string())
                })?;
                visit(event);
            }
            Ok(false)
        })?;
    }
    let expected = range
        .end_inclusive
        .get()
        .checked_sub(range.start.get())
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| JournalError::InvalidRange("revision range overflow".to_string()))?;
    if observed != expected {
        return Err(JournalError::InvalidRange(
            "journal does not contain the complete requested range".to_string(),
        ));
    }
    Ok(())
}

fn segment_window(segments: &[SegmentInfo], range: ThreadRevisionRange) -> &[SegmentInfo] {
    let start = segments.partition_point(|segment| segment.last_revision < range.start);
    let end = segments.partition_point(|segment| segment.first_revision <= range.end_inclusive);
    &segments[start..end]
}

#[cfg(test)]
mod boundary_tests {
    use super::segment_window;
    use crate::SegmentInfo;
    use crate::ThreadRevisionRange;
    use praxis_thread_store_contracts::ThreadRevision;
    use std::path::PathBuf;

    fn segment(sequence: u64, first_revision: u64, last_revision: u64) -> SegmentInfo {
        SegmentInfo {
            sequence,
            first_revision: ThreadRevision::new(first_revision),
            last_revision: ThreadRevision::new(last_revision),
            path: PathBuf::new(),
            bytes: 0,
        }
    }

    #[test]
    fn segment_window_keeps_only_segments_that_can_overlap_the_range() {
        let segments = [
            segment(0, 1, 3),
            segment(1, 4, 8),
            segment(2, 9, 14),
            segment(3, 15, 20),
        ];

        for (start, end, expected) in [
            (1, 1, &[1][..]),
            (2, 3, &[1][..]),
            (4, 8, &[4][..]),
            (7, 10, &[4, 9][..]),
            (15, 20, &[15][..]),
        ] {
            let range = ThreadRevisionRange::inclusive(
                ThreadRevision::new(start),
                ThreadRevision::new(end),
            )
            .expect("valid range");
            let actual = segment_window(&segments, range)
                .iter()
                .map(|segment| segment.first_revision.get())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn decoded_range_events_move_through_the_shared_visitor() {
        let source = include_str!("projection.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("range projection production source");
        assert!(source.contains("impl FnMut(ThreadEventEnvelope)"));
        assert!(source.contains("frame.stored.into_events()"));
        assert_eq!(source.matches("partition_point(").count(), 2);
        assert!(source.contains("for segment in segment_window(segments, range)"));
        assert!(source.contains("visit_segment_pointers(segment"));
        assert!(!source.contains("pointer.end_revision"));
        assert!(!source.contains("event.clone()"));
        assert!(!source.contains("frames:"));
    }
}

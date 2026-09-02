use crate::JournalConfig;
use crate::JournalError;
use crate::SegmentInfo;
use crate::ThreadRevisionRange;
use crate::recovery::FramePointer;
use crate::recovery::FrameReadCursor;
use crate::recovery::read_pointer;
use crate::recovery::segment_index_for_revision;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadHead;
use praxis_thread_store_contracts::ThreadId;
use std::fs::File;

pub(crate) fn fold_range<S>(
    config: &JournalConfig,
    thread_id: ThreadId,
    head: ThreadHead,
    segments: &[SegmentInfo],
    frames: &[FramePointer],
    range: ThreadRevisionRange,
    mut state: S,
    mut fold: impl FnMut(&mut S, &ThreadEventEnvelope),
) -> Result<S, JournalError> {
    visit_range(config, thread_id, head, segments, frames, range, |event| {
        fold(&mut state, &event)
    })?;
    Ok(state)
}

fn visit_range(
    config: &JournalConfig,
    thread_id: ThreadId,
    head: ThreadHead,
    segments: &[SegmentInfo],
    frames: &[FramePointer],
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
    let mut open_segment = None;
    let mut open_segment_index = usize::MAX;
    let mut cursor = FrameReadCursor::default();
    for pointer in frame_window(frames, range) {
        let pointer_segment_is_open = segments.get(open_segment_index).is_some_and(|segment| {
            segment.first_revision <= pointer.start_revision
                && pointer.start_revision <= segment.last_revision
        });
        if !pointer_segment_is_open {
            open_segment_index = segment_index_for_revision(segments, pointer.start_revision)
                .ok_or_else(|| {
                    JournalError::InvalidRange("frame references a missing segment".to_string())
                })?;
            let segment = &segments[open_segment_index];
            open_segment = Some(File::open(&segment.path)?);
            cursor.reset_position();
        }
        let segment = &segments[open_segment_index];
        let frame = read_pointer(
            pointer,
            open_segment.as_mut().ok_or_else(|| {
                JournalError::InvalidRange("segment file was not opened".to_string())
            })?,
            &mut cursor,
            &segment.path,
            thread_id,
            config.max_frame_payload_bytes,
        )?;
        for event in
            frame.stored.into_events().into_iter().filter(|event| {
                event.revision >= range.start && event.revision <= range.end_inclusive
            })
        {
            let expected_revision = range
                .start
                .checked_advance(observed)
                .ok_or_else(|| JournalError::InvalidRange("revision range overflow".to_string()))?;
            if event.revision != expected_revision {
                return Err(JournalError::InvalidRange(
                    "journal does not contain the complete requested range".to_string(),
                ));
            }
            observed = observed
                .checked_add(1)
                .ok_or_else(|| JournalError::InvalidRange("revision range overflow".to_string()))?;
            visit(event);
        }
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

fn frame_window(frames: &[FramePointer], range: ThreadRevisionRange) -> &[FramePointer] {
    let start = frames
        .partition_point(|frame| frame.start_revision <= range.start)
        .saturating_sub(1);
    let end = frames.partition_point(|frame| frame.start_revision <= range.end_inclusive);
    &frames[start..end]
}

#[cfg(test)]
mod boundary_tests {
    use super::frame_window;
    use crate::ThreadRevisionRange;
    use crate::recovery::FramePointer;
    use praxis_thread_store_contracts::Digest;
    use praxis_thread_store_contracts::ThreadRevision;

    fn pointer(start_revision: u64) -> FramePointer {
        FramePointer {
            previous_record_digest: Digest::ZERO,
            start_revision: ThreadRevision::new(start_revision),
            offset: 0,
            len: 0,
        }
    }

    #[test]
    fn frame_window_keeps_only_batches_that_can_overlap_the_range() {
        let frames = [pointer(1), pointer(4), pointer(9), pointer(15)];

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
            let actual = frame_window(&frames, range)
                .iter()
                .map(|frame| frame.start_revision.get())
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
        assert!(source.contains("for pointer in frame_window(frames, range)"));
        assert!(!source.contains("pointer.end_revision"));
        assert!(!source.contains("event.clone()"));
        assert!(!source.contains("frames.iter().enumerate()"));
    }
}

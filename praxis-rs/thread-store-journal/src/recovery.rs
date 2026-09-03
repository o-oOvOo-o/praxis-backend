use crate::JournalConfig;
use crate::JournalError;
use crate::SegmentInfo;
use crate::command_index::CommandIndex;
use crate::format::DecodedFrame;
use crate::format::FRAME_HEADER_LEN;
use crate::format::SEGMENT_HEADER_LEN;
use crate::format::decode_complete_frame;
use crate::format::decode_frame_header;
use crate::format::decode_segment_header;
use crate::format::frame_len;
use crate::format::validate_stored_batch;
use crate::io::sync_directory;
use praxis_thread_store_contracts::CommandId;
use praxis_thread_store_contracts::Digest;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadHead;
use praxis_thread_store_contracts::ThreadId;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub(crate) struct FramePointer {
    pub previous_record_digest: Digest,
    pub start_revision: praxis_thread_store_contracts::ThreadRevision,
    pub offset: u64,
    pub len: u64,
}

impl FramePointer {
    fn previous_head(&self) -> ThreadHead {
        ThreadHead {
            revision: praxis_thread_store_contracts::ThreadRevision::new(
                self.start_revision.get().saturating_sub(1),
            ),
            record_digest: self.previous_record_digest,
        }
    }
}

pub(crate) struct RecoveryState {
    pub head: ThreadHead,
    pub segments: Vec<SegmentInfo>,
    pub command_index: CommandIndex,
}

#[derive(Default)]
pub(crate) struct FrameReadCursor {
    offset: u64,
    bytes: Vec<u8>,
}

pub(crate) fn segment_index_for_revision(
    segments: &[SegmentInfo],
    revision: praxis_thread_store_contracts::ThreadRevision,
) -> Option<usize> {
    let index = segments.partition_point(|segment| segment.last_revision < revision);
    segments
        .get(index)
        .filter(|segment| segment.first_revision <= revision)
        .map(|_| index)
}

impl FrameReadCursor {
    pub fn reset_position(&mut self) {
        self.offset = 0;
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RecoveryMode {
    Writer,
    Snapshot,
}

impl RecoveryMode {
    const fn is_writer(self) -> bool {
        matches!(self, Self::Writer)
    }
}

pub(crate) fn recover_fold<S>(
    config: &JournalConfig,
    thread_id: ThreadId,
    segments_dir: &Path,
    mut state: S,
    mut fold: impl FnMut(&mut S, ThreadEventEnvelope),
) -> Result<(RecoveryState, S), JournalError> {
    let mut visit = |event| fold(&mut state, event);
    let recovered = recover_with_mode(
        config,
        thread_id,
        segments_dir,
        RecoveryMode::Writer,
        &mut visit,
    )?;
    drop(visit);
    Ok((recovered, state))
}

pub(crate) fn recover_snapshot_consume<S>(
    config: &JournalConfig,
    thread_id: ThreadId,
    segments_dir: &Path,
    mut state: S,
    mut consume: impl FnMut(&mut S, ThreadEventEnvelope),
) -> Result<S, JournalError> {
    let mut visit = |event| consume(&mut state, event);
    recover_with_mode(
        config,
        thread_id,
        segments_dir,
        RecoveryMode::Snapshot,
        &mut visit,
    )?;
    drop(visit);
    Ok(state)
}

fn recover_with_mode(
    config: &JournalConfig,
    thread_id: ThreadId,
    segments_dir: &Path,
    mode: RecoveryMode,
    visit: &mut impl FnMut(ThreadEventEnvelope),
) -> Result<RecoveryState, JournalError> {
    let paths = segment_paths(segments_dir)?;
    let path_count = paths.len();
    let mut state = RecoveryState {
        head: ThreadHead::EMPTY,
        segments: if mode.is_writer() {
            Vec::with_capacity(path_count)
        } else {
            Vec::new()
        },
        command_index: CommandIndex::new(),
    };
    let mut decode_buffer = Vec::new();
    for (index, (sequence, path)) in paths.into_iter().enumerate() {
        let expected_sequence =
            u64::try_from(index).map_err(|_| corrupt(&path, 0, "segment count exceeds u64"))?;
        if sequence != expected_sequence {
            return Err(corrupt(&path, 0, "segment sequence has a gap"));
        }
        if index + 1 == path_count && std::fs::metadata(&path)?.len() < SEGMENT_HEADER_LEN as u64 {
            if mode.is_writer() {
                std::fs::remove_file(&path)?;
                sync_directory(segments_dir)?;
            }
            break;
        }
        recover_segment(
            config,
            thread_id,
            path,
            sequence,
            index + 1 == path_count,
            mode,
            &mut state,
            &mut decode_buffer,
            visit,
        )?;
    }
    Ok(state)
}

pub(crate) fn read_pointer(
    pointer: &FramePointer,
    file: &mut File,
    cursor: &mut FrameReadCursor,
    path: &Path,
    thread_id: ThreadId,
    max_payload_bytes: u64,
) -> Result<DecodedFrame, JournalError> {
    if cursor.offset != pointer.offset {
        file.seek(SeekFrom::Start(pointer.offset))?;
    }
    let len = usize::try_from(pointer.len).map_err(|_| JournalError::CorruptSegment {
        path: path.to_path_buf(),
        offset: pointer.offset,
        reason: "frame length exceeds addressable memory".to_string(),
    })?;
    cursor.bytes.resize(len, 0);
    file.read_exact(&mut cursor.bytes)?;
    cursor.offset =
        pointer
            .offset
            .checked_add(pointer.len)
            .ok_or_else(|| JournalError::CorruptSegment {
                path: path.to_path_buf(),
                offset: pointer.offset,
                reason: "frame end offset overflow".to_string(),
            })?;
    let frame =
        decode_complete_frame(&cursor.bytes).map_err(|reason| JournalError::CorruptSegment {
            path: path.to_path_buf(),
            offset: pointer.offset,
            reason,
        })?;
    if frame.header.payload_len > max_payload_bytes {
        return Err(JournalError::CorruptSegment {
            path: path.to_path_buf(),
            offset: pointer.offset,
            reason: "frame payload exceeds configured maximum".to_string(),
        });
    }
    validate_stored_batch(
        &frame.header,
        &frame.stored,
        thread_id,
        pointer.previous_head(),
    )
    .map_err(|reason| JournalError::CorruptSegment {
        path: path.to_path_buf(),
        offset: pointer.offset,
        reason,
    })?;
    Ok(frame)
}

pub(crate) fn find_command_pointer(
    segments: &[SegmentInfo],
    command_id: CommandId,
) -> Result<Option<FramePointer>, JournalError> {
    let expected = *command_id.as_uuid().as_bytes();
    for segment in segments {
        let mut found = None;
        visit_segment_pointers(segment, |pointer, stored_command_id, _| {
            if stored_command_id == expected {
                found = Some(pointer);
                return Ok(true);
            }
            Ok(false)
        })?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

pub(crate) fn visit_segment_pointers(
    segment: &SegmentInfo,
    mut visit: impl FnMut(FramePointer, [u8; 16], &mut File) -> Result<bool, JournalError>,
) -> Result<bool, JournalError> {
    let mut file = File::open(&segment.path)?;
    let mut offset = SEGMENT_HEADER_LEN as u64;
    while offset < segment.bytes {
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = [0; FRAME_HEADER_LEN];
        file.read_exact(&mut bytes)?;
        let header =
            decode_frame_header(&bytes).map_err(|reason| corrupt(&segment.path, offset, reason))?;
        let len = frame_len(&header).map_err(|reason| corrupt(&segment.path, offset, reason))?;
        let next = offset
            .checked_add(len)
            .ok_or_else(|| corrupt(&segment.path, offset, "segment offset overflow"))?;
        if next > segment.bytes {
            return Err(corrupt(
                &segment.path,
                offset,
                "frame extends beyond recovered segment length",
            ));
        }
        let pointer = FramePointer {
            previous_record_digest: header.previous_record_digest,
            start_revision: header.start_revision,
            offset,
            len,
        };
        if visit(pointer, header.command_id, &mut file)? {
            return Ok(true);
        }
        offset = next;
    }
    if offset != segment.bytes {
        return Err(corrupt(
            &segment.path,
            offset,
            "segment framing does not end at recovered length",
        ));
    }
    Ok(false)
}

fn recovered_command_exists(
    state: &RecoveryState,
    current_path: &Path,
    current_offset: u64,
    command_id: CommandId,
) -> Result<bool, JournalError> {
    if !state.command_index.maybe_contains(command_id) {
        return Ok(false);
    }
    if state.command_index.recent_frame(command_id).is_some()
        || find_command_pointer(&state.segments, command_id)?.is_some()
    {
        return Ok(true);
    }
    command_exists_in_prefix(current_path, current_offset, command_id)
}

fn command_exists_in_prefix(
    path: &Path,
    end_offset: u64,
    command_id: CommandId,
) -> Result<bool, JournalError> {
    let expected = *command_id.as_uuid().as_bytes();
    let mut file = File::open(path)?;
    let mut offset = SEGMENT_HEADER_LEN as u64;
    while offset < end_offset {
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = [0; FRAME_HEADER_LEN];
        file.read_exact(&mut bytes)?;
        let header = decode_frame_header(&bytes).map_err(|reason| corrupt(path, offset, reason))?;
        let len = frame_len(&header).map_err(|reason| corrupt(path, offset, reason))?;
        if header.command_id == expected {
            return Ok(true);
        }
        offset = offset
            .checked_add(len)
            .ok_or_else(|| corrupt(path, offset, "segment offset overflow"))?;
        if offset > end_offset {
            return Err(corrupt(
                path,
                offset,
                "frame crosses the recovered prefix boundary",
            ));
        }
    }
    Ok(false)
}

fn recover_segment(
    config: &JournalConfig,
    thread_id: ThreadId,
    path: PathBuf,
    expected_sequence: u64,
    is_last: bool,
    mode: RecoveryMode,
    state: &mut RecoveryState,
    decode_buffer: &mut Vec<u8>,
    visit: &mut impl FnMut(ThreadEventEnvelope),
) -> Result<(), JournalError> {
    let mut file = if mode.is_writer() {
        OpenOptions::new().read(true).write(true).open(&path)?
    } else {
        File::open(&path)?
    };
    let mut file_len = file.metadata()?.len();
    if file_len < SEGMENT_HEADER_LEN as u64 {
        return Err(corrupt(&path, 0, "incomplete segment header"));
    }
    let mut header_bytes = [0; SEGMENT_HEADER_LEN];
    file.read_exact(&mut header_bytes)?;
    let header =
        decode_segment_header(&header_bytes).map_err(|reason| corrupt(&path, 0, reason))?;
    let expected_first = state
        .head
        .revision
        .checked_next()
        .ok_or_else(|| corrupt(&path, 0, "thread revision overflow"))?;
    if header.sequence != expected_sequence
        || header.first_revision != expected_first
        || header.thread_id != *thread_id.as_uuid().as_bytes()
        || header.previous_record_digest != state.head.record_digest
    {
        return Err(corrupt(
            &path,
            0,
            "segment header breaks journal continuity",
        ));
    }

    let first_revision = header.first_revision;
    let mut offset = SEGMENT_HEADER_LEN as u64;
    while offset < file_len {
        let remaining = file_len - offset;
        if remaining < FRAME_HEADER_LEN as u64 {
            handle_incomplete_tail(&mut file, &path, offset, is_last, mode)?;
            file_len = offset;
            break;
        }
        let mut frame_header_bytes = [0; FRAME_HEADER_LEN];
        file.read_exact(&mut frame_header_bytes)?;
        let frame_header = decode_frame_header(&frame_header_bytes)
            .map_err(|reason| corrupt(&path, offset, reason))?;
        if frame_header.payload_len > config.max_frame_payload_bytes {
            return Err(corrupt(
                &path,
                offset,
                "frame payload exceeds configured maximum",
            ));
        }
        let length = frame_len(&frame_header).map_err(|reason| corrupt(&path, offset, reason))?;
        if length > remaining {
            handle_incomplete_tail(&mut file, &path, offset, is_last, mode)?;
            file_len = offset;
            break;
        }
        let length_usize = usize::try_from(length)
            .map_err(|_| corrupt(&path, offset, "frame length exceeds addressable memory"))?;
        decode_buffer.resize(length_usize, 0);
        decode_buffer[..FRAME_HEADER_LEN].copy_from_slice(&frame_header_bytes);
        file.read_exact(&mut decode_buffer[FRAME_HEADER_LEN..])?;
        let decoded = decode_complete_frame(decode_buffer)
            .map_err(|reason| corrupt(&path, offset, reason))?;
        let previous_head = state.head;
        state.head =
            validate_stored_batch(&decoded.header, &decoded.stored, thread_id, previous_head)
                .map_err(|reason| corrupt(&path, offset, reason))?;
        if mode.is_writer() {
            let command_id = decoded.stored.command_id();
            if recovered_command_exists(state, &path, offset, command_id)? {
                return Err(corrupt(&path, offset, "duplicate committed command id"));
            }
            state.command_index.insert(
                command_id,
                FramePointer {
                    previous_record_digest: previous_head.record_digest,
                    start_revision: decoded.header.start_revision,
                    offset,
                    len: length,
                },
            );
        }
        decoded
            .stored
            .into_events()
            .into_iter()
            .for_each(&mut *visit);
        offset = offset
            .checked_add(length)
            .ok_or_else(|| corrupt(&path, offset, "segment offset overflow"))?;
    }
    if !is_last && offset == SEGMENT_HEADER_LEN as u64 {
        return Err(corrupt(&path, offset, "empty segment before journal tail"));
    }
    if mode.is_writer() {
        state.segments.push(SegmentInfo {
            sequence: expected_sequence,
            first_revision,
            last_revision: state.head.revision,
            path,
            bytes: file_len,
        });
    }
    Ok(())
}

fn handle_incomplete_tail(
    file: &mut File,
    path: &Path,
    offset: u64,
    is_last: bool,
    mode: RecoveryMode,
) -> Result<(), JournalError> {
    if !is_last {
        return Err(corrupt(
            path,
            offset,
            "incomplete frame before the final segment",
        ));
    }
    if mode.is_writer() {
        file.set_len(offset)?;
        file.sync_all()?;
    }
    Ok(())
}

fn segment_paths(directory: &Path) -> Result<Vec<(u64, PathBuf)>, JournalError> {
    let mut numbered = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(corrupt(entry.path(), 0, "segment filename is not UTF-8"));
        };
        let Some(sequence) = name
            .strip_prefix("segment-")
            .and_then(|value| value.strip_suffix(".ptj"))
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        numbered.push((sequence, entry.path()));
    }
    numbered.sort_by_key(|(sequence, _)| *sequence);
    Ok(numbered)
}

fn corrupt(path: impl Into<PathBuf>, offset: u64, reason: impl Into<String>) -> JournalError {
    JournalError::CorruptSegment {
        path: path.into(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::FramePointer;
    use super::segment_index_for_revision;
    use crate::SegmentInfo;
    use praxis_thread_store_contracts::ThreadRevision;
    use std::path::PathBuf;

    fn segment(sequence: u64, first: u64, last: u64) -> SegmentInfo {
        SegmentInfo {
            sequence,
            first_revision: ThreadRevision::new(first),
            last_revision: ThreadRevision::new(last),
            path: PathBuf::from(format!("segment-{sequence}.ptj")),
            bytes: 0,
        }
    }

    #[test]
    fn resident_frame_pointer_stays_compact() {
        assert!(std::mem::size_of::<FramePointer>() <= 56);
        let definition = include_str!("recovery.rs")
            .split("pub(crate) struct FramePointer")
            .nth(1)
            .expect("frame pointer definition")
            .split("impl FramePointer")
            .next()
            .expect("frame pointer fields");
        assert!(!definition.contains("end_revision"));
        assert!(!definition.contains("segment_index"));
    }

    #[test]
    fn revision_lookup_derives_segment_without_resident_frame_duplication() {
        let segments = [segment(0, 1, 3), segment(1, 4, 8), segment(2, 10, 12)];

        for (revision, expected) in [
            (1, Some(0)),
            (3, Some(0)),
            (4, Some(1)),
            (8, Some(1)),
            (9, None),
            (10, Some(2)),
            (12, Some(2)),
            (13, None),
        ] {
            assert_eq!(
                segment_index_for_revision(&segments, ThreadRevision::new(revision)),
                expected,
                "revision {revision}"
            );
        }
    }

    #[test]
    fn recovery_has_no_unused_non_folding_writer_entry_point() {
        let writer_surface = include_str!("recovery.rs")
            .lines()
            .take(110)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!writer_surface.contains("pub(crate) fn recover("));
    }

    #[test]
    fn recovery_owns_one_decode_buffer_across_all_segments() {
        let source = include_str!("recovery.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("recovery production source");
        assert_eq!(
            source.matches("let mut decode_buffer = Vec::new()").count(),
            1
        );
        let segment = source
            .split("fn recover_segment")
            .nth(1)
            .expect("segment recovery")
            .split("fn handle_incomplete_tail")
            .next()
            .expect("segment recovery body");
        assert!(segment.contains("decode_buffer: &mut Vec<u8>"));
        assert!(!segment.contains("let mut bytes = Vec::new()"));
    }
}

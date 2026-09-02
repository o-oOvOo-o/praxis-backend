use crate::AppendOutcome;
use crate::DurabilityBarrier;
use crate::JournalBatch;
use crate::JournalConfig;
use crate::JournalDurability;
use crate::JournalError;
use crate::SegmentInfo;
use crate::ThreadRevisionRange;
use crate::format::EncodedFrame;
use crate::format::SEGMENT_HEADER_LEN;
use crate::format::encode_batch;
use crate::format::encode_segment_header;
use crate::format::validate_stored_batch;
use crate::io::AppendError;
use crate::io::append_at;
use crate::io::sync_directory;
use crate::io::sync_file;
use crate::io::truncate_and_sync;
use crate::projection;
use crate::recovery::FramePointer;
use crate::recovery::FrameReadCursor;
use crate::recovery::read_pointer;
use crate::recovery::recover_fold;
use fs2::FileExt;
use praxis_thread_store_contracts::CommandId;
use praxis_thread_store_contracts::ThreadCommandReceipt;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadHead;
use praxis_thread_store_contracts::ThreadId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

pub struct ThreadJournal {
    config: JournalConfig,
    thread_id: ThreadId,
    segments_dir: PathBuf,
    lock_path: PathBuf,
    writer_lock: File,
    head: ThreadHead,
    segments: Vec<SegmentInfo>,
    frames: Vec<FramePointer>,
    command_frames: HashMap<CommandId, usize>,
    pending_syncs: PendingSegmentSyncs,
    dirty_batch_count: u64,
}

#[derive(Default)]
struct PendingSegmentSyncs {
    sequences: Vec<u64>,
}

impl PendingSegmentSyncs {
    fn mark(&mut self, sequence: u64) {
        match self.sequences.last().copied() {
            None => {
                self.sequences.push(sequence);
                return;
            }
            Some(last) if last == sequence => return,
            Some(last) if last < sequence => {
                self.sequences.push(sequence);
                return;
            }
            Some(_) => {}
        }
        if let Err(index) = self.sequences.binary_search(&sequence) {
            self.sequences.insert(index, sequence);
        }
    }

    fn iter_including(&self, current: Option<u64>) -> impl Iterator<Item = u64> + '_ {
        self.sequences
            .iter()
            .copied()
            .chain(current.filter(|sequence| self.sequences.binary_search(sequence).is_err()))
    }

    fn clear(&mut self) {
        self.sequences.clear();
    }

    fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }
}

impl ThreadJournal {
    /// Canonical directory containing this thread's journal segments and writer lock.
    pub fn directory(&self) -> &Path {
        self.lock_path
            .parent()
            .expect("journal writer lock always has a parent directory")
    }

    pub fn open(config: JournalConfig, thread_id: ThreadId) -> Result<Self, JournalError> {
        Self::open_and_fold(config, thread_id, (), |_, _| {}).map(|(journal, ())| journal)
    }

    /// Recover writer state and transfer decoded events without a second journal scan.
    pub fn open_and_fold<S>(
        config: JournalConfig,
        thread_id: ThreadId,
        state: S,
        fold: impl FnMut(&mut S, ThreadEventEnvelope),
    ) -> Result<(Self, S), JournalError> {
        validate_config(&config)?;
        let journal_dir = config
            .root
            .join("threads")
            .join(thread_id.to_string())
            .join("journal");
        let segments_dir = journal_dir.join("segments");
        std::fs::create_dir_all(&segments_dir)?;
        let lock_path = journal_dir.join(".writer.lock");
        if !process_writer_paths()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(lock_path.clone())
        {
            return Err(JournalError::WriterBusy);
        }
        let writer_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .inspect_err(|_| release_process_writer(&lock_path))?;
        match writer_lock.try_lock_exclusive() {
            Ok(()) => {}
            Err(error) if is_writer_busy_error(&error) => {
                release_process_writer(&lock_path);
                return Err(JournalError::WriterBusy);
            }
            Err(error) => {
                release_process_writer(&lock_path);
                return Err(error.into());
            }
        }
        let (recovered, state) = recover_fold(&config, thread_id, &segments_dir, state, fold)
            .inspect_err(|_| {
                let _ = FileExt::unlock(&writer_lock);
                release_process_writer(&lock_path);
            })?;
        Ok((
            Self {
                config,
                thread_id,
                segments_dir,
                lock_path,
                writer_lock,
                head: recovered.head,
                segments: recovered.segments,
                frames: recovered.frames,
                command_frames: recovered.command_frames,
                pending_syncs: PendingSegmentSyncs::default(),
                dirty_batch_count: 0,
            },
            state,
        ))
    }

    pub const fn head(&self) -> ThreadHead {
        self.head
    }

    /// Reports whether another live writer currently owns this journal.
    /// The probe never creates a journal and releases its temporary lock before
    /// returning, so read-only discovery stays side-effect free.
    pub fn writer_is_busy(
        config: &JournalConfig,
        thread_id: ThreadId,
    ) -> Result<bool, JournalError> {
        validate_config(config)?;
        let lock_path = config
            .root
            .join("threads")
            .join(thread_id.to_string())
            .join("journal")
            .join(".writer.lock");
        if process_writer_paths()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&lock_path)
        {
            return Ok(true);
        }
        let writer_lock = match OpenOptions::new().read(true).write(true).open(&lock_path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        match writer_lock.try_lock_exclusive() {
            Ok(()) => {
                FileExt::unlock(&writer_lock)?;
                Ok(false)
            }
            Err(error) if is_writer_busy_error(&error) => Ok(true),
            Err(error) => Err(error.into()),
        }
    }

    pub const fn needs_sync(&self) -> bool {
        self.dirty_batch_count > 0
    }

    pub fn append(
        &mut self,
        batch: JournalBatch,
        durability: JournalDurability,
    ) -> Result<AppendOutcome, JournalError> {
        if let Some(&frame_index) = self.command_frames.get(&batch.command_id) {
            let (command_digest, receipt) = self.read_committed_command(frame_index)?;
            if command_digest == batch.command_digest {
                return Ok(AppendOutcome::Duplicate(receipt));
            }
            return Err(JournalError::IdempotencyCollision {
                command_id: batch.command_id,
            });
        }
        if batch.thread_id != self.thread_id {
            return Err(JournalError::InvalidBatch(
                "batch thread id does not match the open journal".to_string(),
            ));
        }
        if batch.expected_revision != self.head.revision {
            return Err(JournalError::RevisionConflict {
                expected: batch.expected_revision,
                current: self.head.revision,
            });
        }
        let command_id = batch.command_id;
        let encoded = encode_batch(batch, durability, self.config.max_frame_payload_bytes)
            .map_err(JournalError::InvalidBatch)?;
        let next_head =
            validate_stored_batch(&encoded.header, &encoded.stored, self.thread_id, self.head)
                .map_err(JournalError::InvalidBatch)?;
        let segment_index = self.select_segment(encoded.bytes.len())?;
        let segment_sequence = self.segments[segment_index].sequence;
        let before = self.segments[segment_index].bytes;
        write_frame(&self.segments[segment_index].path, before, &encoded)?;

        if durability == JournalDurability::Durable {
            if let Err(error) = self.sync_pending_segments(Some(segment_sequence)) {
                rollback_frame(&self.segments[segment_index].path, before)?;
                return Err(error);
            }
            self.pending_syncs.clear();
            self.dirty_batch_count = 0;
        } else {
            self.pending_syncs.mark(segment_sequence);
            self.dirty_batch_count = self.dirty_batch_count.checked_add(1).ok_or_else(|| {
                JournalError::InvalidBatch("dirty batch count overflow".to_string())
            })?;
        }

        let offset = before;
        let len = u64::try_from(encoded.bytes.len())
            .map_err(|_| JournalError::InvalidBatch("frame length exceeds u64".to_string()))?;
        self.segments[segment_index].bytes = before
            .checked_add(len)
            .ok_or_else(|| JournalError::InvalidBatch("segment length overflow".to_string()))?;
        self.segments[segment_index].last_revision = next_head.revision;
        let frame = FramePointer {
            previous_record_digest: self.head.record_digest,
            start_revision: encoded.header.start_revision,
            offset,
            len,
        };
        let frame_index = self.frames.len();
        self.frames.push(frame);
        self.head = next_head;
        let (receipt, events) = encoded
            .stored
            .into_receipt_and_events()
            .map_err(JournalError::InvalidBatch)?;
        self.command_frames.insert(command_id, frame_index);
        Ok(AppendOutcome::Committed { receipt, events })
    }

    pub fn sync(&mut self) -> Result<DurabilityBarrier, JournalError> {
        let count = self.dirty_batch_count;
        if count > 0 {
            self.sync_pending_segments(None)?;
            self.pending_syncs.clear();
            self.dirty_batch_count = 0;
        }
        Ok(DurabilityBarrier {
            through: self.head,
            batch_count: count,
        })
    }

    pub fn fold_range<S>(
        &self,
        range: ThreadRevisionRange,
        state: S,
        fold: impl FnMut(&mut S, &ThreadEventEnvelope),
    ) -> Result<S, JournalError> {
        projection::fold_range(
            &self.config,
            self.thread_id,
            self.head,
            &self.segments,
            &self.frames,
            range,
            state,
            fold,
        )
    }

    pub fn segments(&self) -> &[SegmentInfo] {
        &self.segments
    }

    pub fn receipt(
        &self,
        command_id: CommandId,
    ) -> Result<Option<ThreadCommandReceipt>, JournalError> {
        let Some(&frame_index) = self.command_frames.get(&command_id) else {
            return Ok(None);
        };
        let (_, receipt) = self.read_committed_command(frame_index)?;
        Ok(Some(receipt))
    }

    fn read_committed_command(
        &self,
        frame_index: usize,
    ) -> Result<(praxis_thread_store_contracts::Digest, ThreadCommandReceipt), JournalError> {
        let frame = self.frames.get(frame_index).ok_or_else(|| {
            JournalError::InvalidConfig("receipt references a missing frame".to_string())
        })?;
        let segment_index =
            crate::recovery::segment_index_for_revision(&self.segments, frame.start_revision)
                .ok_or_else(|| {
                    JournalError::InvalidConfig(
                        "receipt frame references a missing segment".to_string(),
                    )
                })?;
        let segment = &self.segments[segment_index];
        let mut file = File::open(&segment.path)?;
        let mut cursor = FrameReadCursor::default();
        let decoded = read_pointer(
            frame,
            &mut file,
            &mut cursor,
            &segment.path,
            self.thread_id,
            self.config.max_frame_payload_bytes,
        )?;
        let command_digest = decoded.stored.command_digest();
        let receipt =
            decoded
                .stored
                .into_receipt()
                .map_err(|reason| JournalError::CorruptSegment {
                    path: segment.path.clone(),
                    offset: frame.offset,
                    reason,
                })?;
        Ok((command_digest, receipt))
    }

    fn select_segment(&mut self, frame_bytes: usize) -> Result<usize, JournalError> {
        let frame_bytes = u64::try_from(frame_bytes)
            .map_err(|_| JournalError::InvalidBatch("frame length exceeds u64".to_string()))?;
        let rotate = self.segments.last().is_none_or(|segment| {
            segment.bytes > SEGMENT_HEADER_LEN as u64
                && segment
                    .bytes
                    .checked_add(frame_bytes)
                    .is_none_or(|bytes| bytes > self.config.max_segment_bytes)
        });
        if rotate {
            self.create_segment()?;
        }
        Ok(self.segments.len() - 1)
    }

    fn create_segment(&mut self) -> Result<(), JournalError> {
        let sequence = u64::try_from(self.segments.len())
            .map_err(|_| JournalError::InvalidConfig("segment sequence overflow".to_string()))?;
        let first_revision =
            self.head.revision.checked_next().ok_or_else(|| {
                JournalError::InvalidBatch("thread revision overflow".to_string())
            })?;
        let path = self
            .segments_dir
            .join(format!("segment-{sequence:020}.ptj"));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        let header = encode_segment_header(
            sequence,
            first_revision,
            self.thread_id,
            self.head.record_digest,
        );
        if let Err(error) = write_new_segment_header(&mut file, &header) {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(error.into());
        }
        self.segments.push(SegmentInfo {
            sequence,
            first_revision,
            last_revision: self.head.revision,
            path,
            bytes: SEGMENT_HEADER_LEN as u64,
        });
        Ok(())
    }

    fn sync_pending_segments(&self, current: Option<u64>) -> Result<(), JournalError> {
        for sequence in self.pending_syncs.iter_including(current) {
            let index = usize::try_from(sequence).map_err(|_| {
                JournalError::InvalidConfig("segment sequence exceeds usize".to_string())
            })?;
            let segment = self.segments.get(index).ok_or_else(|| {
                JournalError::InvalidConfig("dirty segment is not open".to_string())
            })?;
            sync_file(&segment.path)?;
        }
        if current.is_some() || !self.pending_syncs.is_empty() {
            sync_directory(&self.segments_dir)?;
        }
        Ok(())
    }
}

fn is_writer_busy_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock || matches!(error.raw_os_error(), Some(32 | 33))
}

fn write_new_segment_header(file: &mut File, header: &[u8]) -> io::Result<()> {
    file.write_all(header)
}

impl Drop for ThreadJournal {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.writer_lock);
        release_process_writer(&self.lock_path);
    }
}

fn process_writer_paths() -> &'static Mutex<HashSet<PathBuf>> {
    static PATHS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    PATHS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn release_process_writer(path: &Path) {
    process_writer_paths()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(path);
}

pub(crate) fn validate_config(config: &JournalConfig) -> Result<(), JournalError> {
    if config.max_segment_bytes <= SEGMENT_HEADER_LEN as u64 {
        return Err(JournalError::InvalidConfig(
            "max segment bytes must exceed the segment header".to_string(),
        ));
    }
    if config.max_frame_payload_bytes == 0 {
        return Err(JournalError::InvalidConfig(
            "max frame payload bytes must be non-zero".to_string(),
        ));
    }
    Ok(())
}

fn write_frame(
    path: &Path,
    expected_offset: u64,
    frame: &EncodedFrame,
) -> Result<(), JournalError> {
    match append_at(path, expected_offset, &frame.bytes) {
        Ok(()) => Ok(()),
        Err(AppendError::OffsetChanged { actual }) => Err(JournalError::CorruptSegment {
            path: path.to_path_buf(),
            offset: actual,
            reason: format!("segment length changed from expected offset {expected_offset}"),
        }),
        Err(AppendError::BeforeWrite(error)) => Err(error.into()),
        Err(AppendError::DuringWrite(error)) => {
            truncate_and_sync(path, expected_offset)?;
            Err(error.into())
        }
    }
}

fn rollback_frame(path: &Path, offset: u64) -> Result<(), JournalError> {
    truncate_and_sync(path, offset)?;
    Ok(())
}

#[cfg(test)]
mod boundary_tests {
    use super::PendingSegmentSyncs;

    #[test]
    fn pending_segment_syncs_are_compact_sorted_and_unique() {
        let mut pending = PendingSegmentSyncs::default();
        for sequence in [2, 2, 4, 3] {
            pending.mark(sequence);
        }
        assert_eq!(
            pending.iter_including(Some(4)).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(
            pending.iter_including(Some(5)).collect::<Vec<_>>(),
            vec![2, 3, 4, 5]
        );
    }

    #[test]
    fn append_syncs_owned_segment_state_without_clone_snapshots() {
        let source = include_str!("journal.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("journal production source");
        assert!(!source.contains("dirty_segments.clone()"));
        assert!(!source.contains("BTreeSet"));
        assert!(!source.contains("let path = self.segments[segment_index].path.clone()"));
        assert!(source.contains("sync_pending_segments(Some(segment_sequence))"));
        assert!(source.contains("pending_syncs.mark(segment_sequence)"));
        assert!(source.contains("pub fn segments(&self) -> &[SegmentInfo]"));
    }
}

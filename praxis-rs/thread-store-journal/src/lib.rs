//! Segmented durable journal for Praxis native thread events.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod command_index;
mod error;
mod format;
mod io;
mod journal;
mod model;
mod projection;
mod recovery;
mod snapshot;

pub use error::JournalError;
pub use journal::ThreadJournal;
pub use journal::ThreadJournalSnapshot;
pub use model::AppendOutcome;
pub use model::DurabilityBarrier;
pub use model::JournalBatch;
pub use model::JournalConfig;
pub use model::JournalDurability;
pub use model::SegmentInfo;
pub use model::ThreadRevisionRange;
pub use snapshot::consume_snapshot;
pub use snapshot::fold_snapshot;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Failure;
    use crate::io::fail_next;
    use praxis_thread_store_contracts::BatchId;
    use praxis_thread_store_contracts::CommandId;
    use praxis_thread_store_contracts::EventId;
    use praxis_thread_store_contracts::NewThreadEvent;
    use praxis_thread_store_contracts::ThreadActor;
    use praxis_thread_store_contracts::ThreadCommand;
    use praxis_thread_store_contracts::ThreadCommandHeader;
    use praxis_thread_store_contracts::ThreadEventBody;
    use praxis_thread_store_contracts::ThreadEventEnvelope;
    use praxis_thread_store_contracts::ThreadHead;
    use praxis_thread_store_contracts::ThreadId;
    use praxis_thread_store_contracts::ThreadRevision;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn thread_id(value: u128) -> ThreadId {
        ThreadId::from_uuid(id(value))
    }

    fn batch(
        thread_id: ThreadId,
        command_seed: u128,
        batch_seed: u128,
        head: ThreadHead,
    ) -> JournalBatch {
        let actor = ThreadActor::User;
        let correlation_id = None;
        let command = ThreadCommand::SetName {
            name: Some(format!("name-{command_seed}")),
        };
        let command = ThreadCommandHeader::new(
            CommandId::from_uuid(id(command_seed)),
            thread_id,
            head.revision,
            &actor,
            &correlation_id,
            &command,
        );
        let batch_id = BatchId::from_uuid(id(batch_seed));
        let event = ThreadEventEnvelope::new(NewThreadEvent {
            thread_id,
            revision: head.revision.checked_next().expect("test revision"),
            event_id: EventId::from_uuid(id(batch_seed + 1)),
            batch_id,
            sequence: 0,
            recorded_at_unix_ms: 1_787_000_000_000,
            actor: ThreadActor::User,
            correlation_id: None,
            causation_id: None,
            body: ThreadEventBody::ThreadNameSet {
                name: Some(format!("name-{command_seed}")),
            },
            previous_record_digest: head.record_digest,
        });
        JournalBatch::new(command, batch_id, 1_787_000_000_000, vec![event])
    }

    fn open(root: &TempDir, thread_id: ThreadId) -> ThreadJournal {
        ThreadJournal::open(
            JournalConfig::new(root.path()).with_max_segment_bytes(16 * 1024),
            thread_id,
        )
        .expect("open journal")
    }

    #[test]
    fn snapshot_reads_committed_prefix_while_writer_remains_open() {
        let root = TempDir::new().expect("tempdir");
        let thread_id = thread_id(90);
        let mut journal = open(&root, thread_id);
        journal
            .append(
                batch(thread_id, 91, 92, journal.head()),
                JournalDurability::Durable,
            )
            .expect("append batch");

        assert_eq!(
            fold_snapshot(
                JournalConfig::new(root.path()),
                thread_id,
                0_usize,
                |count, _| *count += 1,
            )
            .expect("open and fold snapshot"),
            1
        );
    }

    #[test]
    fn snapshot_ignores_incomplete_tail_without_repairing_writer_files() {
        use std::io::Write as _;

        let root = TempDir::new().expect("tempdir");
        let thread_id = thread_id(93);
        let mut journal = open(&root, thread_id);
        journal
            .append(
                batch(thread_id, 94, 95, journal.head()),
                JournalDurability::Durable,
            )
            .expect("append batch");
        let segment = journal.segments()[0].path.clone();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&segment)
            .expect("open segment tail");
        file.write_all(&[1, 2, 3]).expect("append partial frame");
        drop(file);
        let dirty_len = std::fs::metadata(&segment).expect("segment metadata").len();

        assert_eq!(
            fold_snapshot(
                JournalConfig::new(root.path()),
                thread_id,
                0_usize,
                |count, _| *count += 1,
            )
            .expect("fold committed prefix beside incomplete tail"),
            1
        );
        assert_eq!(
            std::fs::metadata(&segment).expect("segment metadata").len(),
            dirty_len
        );
    }

    #[test]
    fn writer_recovery_folds_events_across_segments_during_one_decode() {
        let root = TempDir::new().expect("tempdir");
        let thread_id = thread_id(96);
        let config = JournalConfig::new(root.path()).with_max_segment_bytes(512);
        let mut journal = ThreadJournal::open(config.clone(), thread_id).expect("open journal");
        for seed in 97..100 {
            journal
                .append(
                    batch(thread_id, seed, seed + 10, journal.head()),
                    JournalDurability::Durable,
                )
                .expect("append batch");
        }
        assert!(journal.segments().len() > 1);
        let head = journal.head();
        drop(journal);

        let (reopened, count) =
            ThreadJournal::open_and_fold(config, thread_id, 0_usize, |count, _| *count += 1)
                .expect("reopen and fold journal");
        assert_eq!(reopened.head(), head);
        assert_eq!(count, 3);
    }

    #[test]
    fn range_fold_crosses_frames_and_segments() {
        let root = TempDir::new().expect("tempdir");
        let thread_id = thread_id(130);
        let mut journal = ThreadJournal::open(
            JournalConfig::new(root.path()).with_max_segment_bytes(512),
            thread_id,
        )
        .expect("open small-segment journal");
        for seed in 131..134 {
            journal
                .append(
                    batch(thread_id, seed, seed + 10, journal.head()),
                    JournalDurability::Durable,
                )
                .expect("append projected batch");
        }
        assert!(journal.segments().len() > 1);
        assert_eq!(
            journal
                .fold_range(
                    ThreadRevisionRange::inclusive(ThreadRevision::new(1), ThreadRevision::new(3),)
                        .expect("revision range"),
                    0_usize,
                    |count, _| *count += 1,
                )
                .expect("fold across segments"),
            3,
        );
    }

    #[test]
    fn partial_write_failure_rolls_back_and_retry_commits_once() {
        let root = TempDir::new().expect("tempdir");
        let thread_id = thread_id(100);
        let mut journal = open(&root, thread_id);
        let pending = batch(thread_id, 101, 102, journal.head());

        fail_next(Failure::PartialWrite(23));
        assert!(matches!(
            journal.append(pending.clone(), JournalDurability::Durable),
            Err(JournalError::Io(_))
        ));
        let segment = journal.segments()[0].clone();
        assert_eq!(journal.head(), ThreadHead::EMPTY);
        assert_eq!(
            std::fs::metadata(&segment.path)
                .expect("segment metadata")
                .len(),
            segment.bytes
        );

        journal
            .append(pending, JournalDurability::Durable)
            .expect("retry append");
        assert_eq!(journal.head().revision, ThreadRevision::new(1));
    }

    #[test]
    fn durable_sync_failure_rolls_back_and_retry_commits_once() {
        let root = TempDir::new().expect("tempdir");
        let thread_id = thread_id(110);
        let mut journal = open(&root, thread_id);
        let pending = batch(thread_id, 111, 112, journal.head());

        fail_next(Failure::FileSync);
        assert!(matches!(
            journal.append(pending.clone(), JournalDurability::Durable),
            Err(JournalError::Io(_))
        ));
        let segment = journal.segments()[0].clone();
        assert_eq!(journal.head(), ThreadHead::EMPTY);
        assert_eq!(
            std::fs::metadata(&segment.path)
                .expect("segment metadata")
                .len(),
            segment.bytes
        );

        journal
            .append(pending, JournalDurability::Durable)
            .expect("retry append");
        assert_eq!(journal.head().revision, ThreadRevision::new(1));
    }

    #[test]
    fn barrier_sync_failure_keeps_buffered_batches_pending_for_retry() {
        let root = TempDir::new().expect("tempdir");
        let thread_id = thread_id(120);
        let mut journal = open(&root, thread_id);
        journal
            .append(
                batch(thread_id, 121, 122, journal.head()),
                JournalDurability::Buffered,
            )
            .expect("append buffered batch");
        let committed_head = journal.head();

        fail_next(Failure::FileSync);
        assert!(matches!(journal.sync(), Err(JournalError::Io(_))));
        assert_eq!(journal.head(), committed_head);
        assert!(journal.needs_sync());

        let barrier = journal.sync().expect("retry barrier");
        assert_eq!(barrier.through, committed_head);
        assert_eq!(barrier.batch_count, 1);
        assert!(!journal.needs_sync());
    }
}

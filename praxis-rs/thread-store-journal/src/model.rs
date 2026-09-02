use crate::JournalError;
use praxis_thread_store_contracts::BatchId;
use praxis_thread_store_contracts::CommandId;
use praxis_thread_store_contracts::Digest;
use praxis_thread_store_contracts::ThreadCommandHeader;
use praxis_thread_store_contracts::ThreadCommandReceipt;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadHead;
use praxis_thread_store_contracts::ThreadId;
use praxis_thread_store_contracts::ThreadRevision;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalConfig {
    pub(crate) root: PathBuf,
    pub(crate) max_segment_bytes: u64,
    pub(crate) max_frame_payload_bytes: u64,
}

impl JournalConfig {
    pub const DEFAULT_MAX_SEGMENT_BYTES: u64 = 64 * 1024 * 1024;
    pub const DEFAULT_MAX_FRAME_PAYLOAD_BYTES: u64 = 256 * 1024 * 1024;

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_segment_bytes: Self::DEFAULT_MAX_SEGMENT_BYTES,
            max_frame_payload_bytes: Self::DEFAULT_MAX_FRAME_PAYLOAD_BYTES,
        }
    }

    pub const fn with_max_segment_bytes(mut self, value: u64) -> Self {
        self.max_segment_bytes = value;
        self
    }

    pub const fn with_max_frame_payload_bytes(mut self, value: u64) -> Self {
        self.max_frame_payload_bytes = value;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalDurability {
    Buffered,
    Durable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalBatch {
    pub(crate) thread_id: ThreadId,
    pub(crate) batch_id: BatchId,
    pub(crate) command_id: CommandId,
    pub(crate) command_digest: Digest,
    pub(crate) expected_revision: ThreadRevision,
    pub(crate) recorded_at_unix_ms: i64,
    pub(crate) events: Vec<ThreadEventEnvelope>,
}

impl JournalBatch {
    pub fn new(
        command: ThreadCommandHeader,
        batch_id: BatchId,
        recorded_at_unix_ms: i64,
        events: Vec<ThreadEventEnvelope>,
    ) -> Self {
        Self {
            thread_id: command.thread_id(),
            batch_id,
            command_id: command.command_id(),
            command_digest: command.command_digest(),
            expected_revision: command.expected_revision(),
            recorded_at_unix_ms,
            events,
        }
    }

    pub fn events(&self) -> &[ThreadEventEnvelope] {
        &self.events
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppendOutcome {
    Committed {
        receipt: ThreadCommandReceipt,
        events: Vec<ThreadEventEnvelope>,
    },
    Duplicate(ThreadCommandReceipt),
}

impl AppendOutcome {
    pub const fn receipt(&self) -> &ThreadCommandReceipt {
        match self {
            Self::Committed { receipt, .. } | Self::Duplicate(receipt) => receipt,
        }
    }

    pub fn into_receipt(self) -> ThreadCommandReceipt {
        match self {
            Self::Committed { receipt, .. } | Self::Duplicate(receipt) => receipt,
        }
    }

    pub fn into_receipt_and_events(
        self,
    ) -> (ThreadCommandReceipt, Option<Vec<ThreadEventEnvelope>>) {
        match self {
            Self::Committed { receipt, events } => (receipt, Some(events)),
            Self::Duplicate(receipt) => (receipt, None),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DurabilityBarrier {
    pub through: ThreadHead,
    pub batch_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentInfo {
    pub sequence: u64,
    pub first_revision: ThreadRevision,
    pub last_revision: ThreadRevision,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadRevisionRange {
    pub start: ThreadRevision,
    pub end_inclusive: ThreadRevision,
}

impl ThreadRevisionRange {
    pub fn inclusive(
        start: ThreadRevision,
        end_inclusive: ThreadRevision,
    ) -> Result<Self, JournalError> {
        if start == ThreadRevision::ZERO {
            return Err(JournalError::InvalidRange(
                "revision zero does not identify an event".to_string(),
            ));
        }
        if end_inclusive < start {
            return Err(JournalError::InvalidRange(
                "range end precedes its start".to_string(),
            ));
        }
        Ok(Self {
            start,
            end_inclusive,
        })
    }
}

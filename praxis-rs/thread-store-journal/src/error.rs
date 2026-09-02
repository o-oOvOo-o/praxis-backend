use praxis_thread_store_contracts::CommandId;
use praxis_thread_store_contracts::ThreadRevision;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("invalid thread journal configuration: {0}")]
    InvalidConfig(String),
    #[error("thread journal already has an active writer")]
    WriterBusy,
    #[error("thread revision conflict: expected {expected:?}, current {current:?}")]
    RevisionConflict {
        expected: ThreadRevision,
        current: ThreadRevision,
    },
    #[error("command {command_id} was reused with different canonical content")]
    IdempotencyCollision { command_id: CommandId },
    #[error("invalid thread journal batch: {0}")]
    InvalidBatch(String),
    #[error("invalid thread revision range: {0}")]
    InvalidRange(String),
    #[error("corrupt thread journal segment {path} at byte {offset}: {reason}")]
    CorruptSegment {
        path: PathBuf,
        offset: u64,
        reason: String,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
}

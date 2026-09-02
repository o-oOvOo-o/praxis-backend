use praxis_thread_store_journal::JournalError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThreadStoreError {
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Index(#[from] sqlx::Error),
    #[error("thread has not been created")]
    ThreadNotCreated,
    #[error("thread already exists")]
    ThreadAlreadyExists,
    #[error("thread revision {0} does not exist")]
    RevisionNotFound(u64),
    #[error(
        "prepared projection revision {prepared} is ahead of recovered journal revision {recovered}"
    )]
    PreparedProjectionAhead { prepared: u64, recovered: u64 },
    #[error("ThreadStore worker failed: {0}")]
    Worker(String),
    #[error("ThreadStore writer lock was poisoned")]
    WriterPoisoned,
    #[error("system clock precedes the Unix epoch")]
    InvalidClock,
    #[error("thread revision overflow")]
    RevisionOverflow,
}

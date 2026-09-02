//! Canonical Praxis ThreadStore over the durable segmented journal.

#![forbid(unsafe_code)]

mod error;
mod index;
mod live_thread;
mod projection;
mod store;

pub use error::ThreadStoreError;
pub use live_thread::CommitMode;
pub use live_thread::LiveThreadStore;
pub use live_thread::ThreadSessionMetadata;
pub use projection::NativeTranscriptIndex;
pub use projection::ThreadListPage;
pub use projection::ThreadListQuery;
pub use projection::ThreadListSort;
pub use projection::ThreadSummary;
pub use projection::TranscriptScanPlan;
pub use store::ModelContextFoldCoverage;
pub use store::PreparedThreadFold;
pub use store::PreparedThreadProjection;
pub use store::RecoveredThreadState;
pub use store::ThreadOpenIndex;
pub use store::ThreadStore;

pub const THREAD_STORE_SUBDIR: &str = "thread-store";

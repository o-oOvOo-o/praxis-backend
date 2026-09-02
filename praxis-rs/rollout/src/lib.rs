//! Rollout persistence and discovery for Praxis session files.

use std::sync::LazyLock;

use praxis_protocol::protocol::SessionSource;

pub mod config;
pub mod list;
pub mod metadata;
pub mod policy;
pub mod recorder;
pub mod state_db;
pub mod thread_store;

pub(crate) mod default_client {
    pub use praxis_login::default_client::*;
}

pub(crate) use praxis_protocol::protocol;

pub const SESSIONS_SUBDIR: &str = "sessions";
pub const ARCHIVED_SESSIONS_SUBDIR: &str = "archived_sessions";
pub static INTERACTIVE_SESSION_SOURCES: LazyLock<Vec<SessionSource>> = LazyLock::new(|| {
    vec![
        SessionSource::Cli,
        SessionSource::VSCode,
        SessionSource::AppGateway,
        SessionSource::Custom("atlas".to_string()),
        SessionSource::Custom("chatgpt".to_string()),
    ]
});

pub use config::RolloutConfig;
pub use config::RolloutConfigView;
pub use list::rollout_date_parts;
pub use policy::EventPersistenceMode;
pub use praxis_protocol::protocol::SessionMeta;
pub use praxis_state::ThreadSourceKind;
pub use recorder::RolloutRecorder;
pub use recorder::RolloutRecorderParams;
pub use state_db::StateDbHandle;
pub use thread_store::ListThreadsQuery;
pub use thread_store::ThreadGitInfo;
pub use thread_store::ThreadNameResolver;
pub use thread_store::ThreadNameWriter;
pub use thread_store::ThreadStore;
pub use thread_store::ThreadSummary;
pub use thread_store::ThreadSummaryPage;
pub use thread_store::list_threads;

#[cfg(test)]
mod tests;

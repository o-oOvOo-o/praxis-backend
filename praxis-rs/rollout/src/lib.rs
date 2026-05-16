//! Rollout persistence and discovery for Praxis session files.

use std::sync::LazyLock;

use praxis_protocol::protocol::SessionSource;

pub mod config;
pub mod list;
pub mod metadata;
pub mod policy;
pub mod recorder;
pub mod state_db;
pub mod thread_directory;

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
pub use list::find_archived_thread_path_by_id_str;
pub use list::find_thread_path_by_id_str;
pub use list::rollout_date_parts;
pub use policy::EventPersistenceMode;
pub use praxis_protocol::protocol::SessionMeta;
pub use praxis_state::ThreadSourceKind;
pub use recorder::RolloutRecorder;
pub use recorder::RolloutRecorderParams;
pub use state_db::StateDbHandle;
pub use thread_directory::ListThreadsQuery;
pub use thread_directory::ThreadDirectory;
pub use thread_directory::ThreadGitInfo;
pub use thread_directory::ThreadNameResolver;
pub use thread_directory::ThreadNameWriter;
pub use thread_directory::ThreadSummary;
pub use thread_directory::ThreadSummaryPage;
pub use thread_directory::list_threads;

#[cfg(test)]
mod tests;

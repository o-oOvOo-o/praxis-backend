use crate::SkillsManager;
use crate::agent_os::AgentOs;
#[cfg(test)]
use crate::config::Config;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::praxis_thread::PraxisThread;
use crate::skills_watcher::SkillsWatcher;
use bootstrap::SharedCapturedOps;
use bootstrap::TempPraxisHomeGuard;
#[cfg(test)]
use fork_snapshot::SnapshotTurnState;
#[cfg(test)]
use fork_snapshot::append_interrupted_boundary;
#[cfg(test)]
use fork_snapshot::snapshot_turn_state;
#[cfg(test)]
use fork_snapshot::truncate_before_nth_user_message;
use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::protocol::SessionSource;
#[cfg(test)]
use praxis_protocol::protocol::{
    EventMsg, InitialHistory, RolloutItem, TurnAbortReason, TurnAbortedEvent,
};
use registry::ThreadRegistry;
use std::sync::Arc;
use tokio::sync::broadcast;

mod access;
mod bootstrap;
mod construction;
mod fork;
mod fork_snapshot;
mod inner;
mod lifecycle;
mod mcp_refresh;
mod registry;
mod shutdown;
mod source_inheritance;

pub(crate) use bootstrap::set_thread_manager_test_mode_for_tests;
pub use fork::ThreadForkSnapshot;
pub type ThreadShutdownReport = shutdown::ThreadShutdownReport;

const THREAD_CREATED_CHANNEL_CAPACITY: usize = 1024;
/// Represents a newly created Praxis thread and its first configured-session event.
pub struct ThreadSpawnResult {
    pub thread_id: ThreadId,
    pub thread: Arc<PraxisThread>,
    pub session_configured: SessionConfiguredEvent,
}

/// [`ThreadManager`] is responsible for creating threads and maintaining
/// them in memory.
pub struct ThreadManager {
    state: Arc<ThreadManagerInner>,
    _test_praxis_home_guard: Option<TempPraxisHomeGuard>,
}

/// Shared, `Arc`-owned state for [`ThreadManager`]. This `Arc` is required to have a single
/// `Arc` reference that can be downgraded to by agent control while preventing every single
/// function to require an `Arc<&Self>`.
pub(crate) struct ThreadManagerInner {
    threads: ThreadRegistry,
    thread_created_tx: broadcast::Sender<ThreadId>,
    auth_manager: Arc<AuthManager>,
    models_manager: Arc<ModelsManager>,
    environment_manager: Arc<EnvironmentManager>,
    skills_manager: Arc<SkillsManager>,
    plugins_manager: Arc<PluginsManager>,
    mcp_manager: Arc<McpManager>,
    skills_watcher: Arc<SkillsWatcher>,
    pub(crate) agent_os: Arc<AgentOs>,
    session_source: SessionSource,
    // Captures submitted ops for testing purpose when test mode is enabled.
    ops_log: Option<SharedCapturedOps>,
}

#[cfg(test)]
#[path = "thread_manager_tests.rs"]
mod tests;

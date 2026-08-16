use crate::agent_os::AgentOs;
#[cfg(test)]
use crate::config::Config;
use crate::mcp::McpManager;
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
use praxis_capability_runtime::CapabilityRuntime;
use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::ThreadId;
#[cfg(test)]
use praxis_protocol::protocol::EventMsg;
#[cfg(test)]
use praxis_protocol::protocol::InitialHistory;
#[cfg(test)]
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::protocol::SessionSource;
#[cfg(test)]
use praxis_protocol::protocol::TurnAbortReason;
#[cfg(test)]
use praxis_protocol::protocol::TurnAbortedEvent;
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
mod services;
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
    pub initial_config_snapshot: crate::praxis_thread::ThreadConfigSnapshot,
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
    provider_capability: crate::capabilities::ProviderCapability,
    environment_manager: Arc<EnvironmentManager>,
    capability_runtime: CapabilityRuntime,
    skills_manager: crate::capabilities::SkillsCapability,
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

use std::sync::Arc;

use futures::future::BoxFuture;
use futures::future::Shared;
use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::W3cTraceContext;

use crate::SkillsManager;
use crate::agent::AgentControl;
use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::exec_policy::ExecPolicyManager;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::skills_watcher::SkillsWatcher;

use super::super::Praxis;

pub(crate) type SessionLoopTermination = Shared<BoxFuture<'static, ()>>;

/// Wrapper returned by [`Praxis::spawn`] containing the spawned [`Praxis`] and thread id.
pub struct PraxisSpawnOk {
    pub praxis: Praxis,
    pub thread_id: ThreadId,
}

pub(crate) struct PraxisSpawnArgs {
    pub(crate) config: Config,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) models_manager: Arc<ModelsManager>,
    pub(crate) environment_manager: Arc<EnvironmentManager>,
    pub(crate) skills_manager: Arc<SkillsManager>,
    pub(crate) plugins_manager: Arc<PluginsManager>,
    pub(crate) mcp_manager: Arc<McpManager>,
    pub(crate) skills_watcher: Arc<SkillsWatcher>,
    pub(crate) conversation_history: InitialHistory,
    pub(crate) session_source: SessionSource,
    pub(crate) agent_control: AgentControl,
    pub(crate) agent_os: Arc<AgentOs>,
    pub(crate) dynamic_tools: Vec<DynamicToolSpec>,
    pub(crate) persist_extended_history: bool,
    pub(crate) metrics_service_name: Option<String>,
    pub(crate) inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
    pub(crate) inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
    pub(crate) user_shell_override: Option<shell::Shell>,
    pub(crate) parent_trace: Option<W3cTraceContext>,
}

pub(crate) const SUBMISSION_CHANNEL_CAPACITY: usize = 512;

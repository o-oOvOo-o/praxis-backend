use std::sync::Arc;

use praxis_exec_server::EnvironmentManager;
use praxis_hooks::Hooks;
use praxis_login::AuthManager;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use praxis_rollout::state_db::StateDbHandle;
use tokio::sync::watch;

use crate::SkillsManager;
use crate::agent::AgentControl;
use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::praxis::SessionConfiguration;
use crate::rollout::RolloutRecorder;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::skills_watcher::SkillsWatcher;
use crate::tools::network_approval::NetworkApprovalService;
use crate::unified_exec::UnifiedExecProcessManager;

pub(in crate::praxis::session_startup) struct ServicesBootstrapInput {
    pub(in crate::praxis::session_startup) config: Arc<Config>,
    pub(in crate::praxis::session_startup) auth_manager: Arc<AuthManager>,
    pub(in crate::praxis::session_startup) models_manager: Arc<ModelsManager>,
    pub(in crate::praxis::session_startup) exec_policy: Arc<ExecPolicyManager>,
    pub(in crate::praxis::session_startup) skills_manager: Arc<SkillsManager>,
    pub(in crate::praxis::session_startup) plugins_manager: Arc<PluginsManager>,
    pub(in crate::praxis::session_startup) mcp_manager: Arc<McpManager>,
    pub(in crate::praxis::session_startup) skills_watcher: Arc<SkillsWatcher>,
    pub(in crate::praxis::session_startup) agent_control: AgentControl,
    pub(in crate::praxis::session_startup) agent_os: Arc<AgentOs>,
    pub(in crate::praxis::session_startup) environment_manager: Arc<EnvironmentManager>,
    pub(in crate::praxis::session_startup) conversation_id: ThreadId,
    pub(in crate::praxis::session_startup) session_configuration: SessionConfiguration,
    pub(in crate::praxis::session_startup) hooks: Hooks,
    pub(in crate::praxis::session_startup) rollout_recorder: Option<RolloutRecorder>,
    pub(in crate::praxis::session_startup) default_shell: Shell,
    pub(in crate::praxis::session_startup) shell_snapshot_tx:
        watch::Sender<Option<Arc<ShellSnapshot>>>,
    pub(in crate::praxis::session_startup) session_telemetry: SessionTelemetry,
    pub(in crate::praxis::session_startup) started_network_proxy: Option<StartedNetworkProxy>,
    pub(in crate::praxis::session_startup) network_approval: Arc<NetworkApprovalService>,
    pub(in crate::praxis::session_startup) state_db_ctx: Option<StateDbHandle>,
    pub(in crate::praxis::session_startup) unified_exec_manager: Arc<UnifiedExecProcessManager>,
}

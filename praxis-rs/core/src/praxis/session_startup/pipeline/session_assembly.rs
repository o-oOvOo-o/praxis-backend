use std::sync::Arc;

use async_channel::Sender;
use praxis_exec_server::EnvironmentManager;
use praxis_hooks::Hooks;
use praxis_login::AuthManager;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_rollout::state_db::StateDbHandle;
use tokio::sync::watch;

use crate::SkillsManager;
use crate::agent::AgentControl;
use crate::agent::AgentStatus;
use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::rollout::RolloutRecorder;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::skills_watcher::SkillsWatcher;
use crate::state::SessionState;
use crate::tools::network_approval::NetworkApprovalService;
use crate::unified_exec::UnifiedExecProcessManager;

use super::super::super::Session;
use super::super::super::SessionConfiguration;
use super::super::network_proxy;
use super::super::services_bootstrap;
use super::super::session_handle;

pub(super) struct SessionAssemblyInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) tx_event: &'a Sender<Event>,
    pub(super) agent_status: watch::Sender<AgentStatus>,
    pub(super) config: &'a Arc<Config>,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(super) auth_manager: &'a Arc<AuthManager>,
    pub(super) models_manager: &'a Arc<ModelsManager>,
    pub(super) exec_policy: Arc<ExecPolicyManager>,
    pub(super) skills_manager: Arc<SkillsManager>,
    pub(super) plugins_manager: &'a Arc<PluginsManager>,
    pub(super) mcp_manager: &'a Arc<McpManager>,
    pub(super) skills_watcher: Arc<SkillsWatcher>,
    pub(super) agent_control: AgentControl,
    pub(super) agent_os: Arc<AgentOs>,
    pub(super) environment_manager: Arc<EnvironmentManager>,
    pub(super) hooks: Hooks,
    pub(super) rollout_recorder: Option<RolloutRecorder>,
    pub(super) default_shell: Shell,
    pub(super) shell_snapshot_tx: watch::Sender<Option<Arc<ShellSnapshot>>>,
    pub(super) session_telemetry: SessionTelemetry,
    pub(super) started_network_proxy: Option<StartedNetworkProxy>,
    pub(super) network_approval: Arc<NetworkApprovalService>,
    pub(super) state_db_ctx: Option<StateDbHandle>,
    pub(super) unified_exec_manager: Arc<UnifiedExecProcessManager>,
    pub(super) network_policy_decider_session: network_proxy::PolicyDeciderSession,
}

pub(super) async fn build(input: SessionAssemblyInput<'_>) -> anyhow::Result<Arc<Session>> {
    let state = SessionState::new(input.session_configuration.clone());
    let services = services_bootstrap::build(services_bootstrap::ServicesBootstrapInput {
        config: Arc::clone(input.config),
        auth_manager: Arc::clone(input.auth_manager),
        models_manager: Arc::clone(input.models_manager),
        exec_policy: input.exec_policy,
        skills_manager: input.skills_manager,
        plugins_manager: Arc::clone(input.plugins_manager),
        mcp_manager: Arc::clone(input.mcp_manager),
        skills_watcher: input.skills_watcher,
        agent_control: input.agent_control,
        agent_os: input.agent_os,
        environment_manager: input.environment_manager,
        conversation_id: input.conversation_id,
        session_configuration: input.session_configuration.clone(),
        hooks: input.hooks,
        rollout_recorder: input.rollout_recorder,
        default_shell: input.default_shell,
        shell_snapshot_tx: input.shell_snapshot_tx,
        session_telemetry: input.session_telemetry,
        started_network_proxy: input.started_network_proxy,
        network_approval: input.network_approval,
        state_db_ctx: input.state_db_ctx,
        unified_exec_manager: input.unified_exec_manager,
    })
    .await?;
    let session = session_handle::build(session_handle::SessionHandleInput {
        conversation_id: input.conversation_id,
        tx_event: input.tx_event.clone(),
        agent_status: input.agent_status,
        config: input.config.as_ref(),
        session_configuration: input.session_configuration,
        state,
        services,
        llm_runtime_catalog: input.llm_runtime_catalog,
    });
    network_proxy::bind_session(input.network_policy_decider_session, &session).await;
    Ok(session)
}

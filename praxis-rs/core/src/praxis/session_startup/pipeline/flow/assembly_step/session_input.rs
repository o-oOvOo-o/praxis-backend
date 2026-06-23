use std::sync::Arc;

use async_channel::Sender;
use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_rollout::state_db::StateDbHandle;
use tokio::sync::watch;

use crate::SkillsManager;
use crate::agent::AgentControl;
use crate::agent::AgentStatus;
use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::exec_policy::ExecPolicyManager;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::rollout::RolloutRecorder;
use crate::skills_watcher::SkillsWatcher;

use super::super::super::super::super::SessionConfiguration;
use super::super::super::session_assembly;
use super::super::super::session_runtime_prepare;

pub(super) struct SessionInputProjection<'a> {
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
    pub(super) runtime: session_runtime_prepare::SessionRuntimePreparation,
    pub(super) rollout_recorder: Option<RolloutRecorder>,
    pub(super) state_db_ctx: Option<StateDbHandle>,
}

pub(super) fn build(
    input: SessionInputProjection<'_>,
) -> session_assembly::SessionAssemblyInput<'_> {
    let session_runtime_prepare::SessionRuntimePreparation {
        session_telemetry,
        default_shell,
        shell_snapshot_tx,
        started_network_proxy,
        session_network_proxy: _,
        network_approval,
        network_policy_decider_session,
        hooks,
        unified_exec_manager,
    } = input.runtime;

    session_assembly::SessionAssemblyInput {
        handle: session_assembly::SessionAssemblyHandle {
            conversation_id: input.conversation_id,
            tx_event: input.tx_event,
            agent_status: input.agent_status,
            config: input.config,
            session_configuration: input.session_configuration,
            llm_runtime_catalog: input.llm_runtime_catalog,
            network_policy_decider_session,
        },
        managers: session_assembly::SessionAssemblyManagers {
            auth_manager: input.auth_manager,
            models_manager: input.models_manager,
            skills_manager: input.skills_manager,
            plugins_manager: input.plugins_manager,
            mcp_manager: input.mcp_manager,
            skills_watcher: input.skills_watcher,
            agent_control: input.agent_control,
            agent_os: input.agent_os,
            environment_manager: input.environment_manager,
        },
        runtime: session_assembly::SessionAssemblyRuntime {
            exec_policy: input.exec_policy,
            hooks,
            rollout_recorder: input.rollout_recorder,
            default_shell,
            shell_snapshot_tx,
            session_telemetry,
            started_network_proxy,
            network_approval,
            state_db_ctx: input.state_db_ctx,
            unified_exec_manager,
        },
    }
}

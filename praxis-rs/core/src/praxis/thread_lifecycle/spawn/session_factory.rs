use std::sync::Arc;

use async_channel::Sender;
use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use tokio::sync::watch;
use tracing::error;

use crate::SkillsManager;
use crate::agent::AgentControl;
use crate::agent::AgentStatus;
use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::exec_policy::ExecPolicyManager;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::rollout::map_session_init_error;
use crate::skills_watcher::SkillsWatcher;

use super::super::super::Session;
use super::super::super::SessionConfiguration;

pub(super) struct SessionFactoryInput {
    pub(super) session_configuration: SessionConfiguration,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(super) config: Arc<Config>,
    pub(super) auth_manager: Arc<AuthManager>,
    pub(super) models_manager: Arc<ModelsManager>,
    pub(super) exec_policy: Arc<ExecPolicyManager>,
    pub(super) tx_event: Sender<Event>,
    pub(super) agent_status_tx: watch::Sender<AgentStatus>,
    pub(super) conversation_history: InitialHistory,
    pub(super) environment_manager: Arc<EnvironmentManager>,
    pub(super) skills_manager: Arc<SkillsManager>,
    pub(super) plugins_manager: Arc<PluginsManager>,
    pub(super) mcp_manager: Arc<McpManager>,
    pub(super) skills_watcher: Arc<SkillsWatcher>,
    pub(super) agent_control: AgentControl,
    pub(super) agent_os: Arc<AgentOs>,
}

pub(super) async fn build(input: SessionFactoryInput) -> PraxisResult<Arc<Session>> {
    let session_source = input.session_configuration.session_source.clone();
    let config_for_error = Arc::clone(&input.config);
    Session::new(
        input.session_configuration,
        input.llm_runtime_catalog,
        input.config,
        input.auth_manager,
        input.models_manager,
        input.exec_policy,
        input.tx_event,
        input.agent_status_tx,
        input.conversation_history,
        session_source,
        input.environment_manager,
        input.skills_manager,
        input.plugins_manager,
        input.mcp_manager,
        input.skills_watcher,
        input.agent_control,
        input.agent_os,
    )
    .await
    .map_err(|e| {
        error!("Failed to create session: {e:#}");
        map_session_init_error(&e, &config_for_error.praxis_home)
    })
}

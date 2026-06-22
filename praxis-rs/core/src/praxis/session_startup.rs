use std::sync::Arc;

use async_channel::Sender;
use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use tokio::sync::watch;
use tracing::instrument;

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
use crate::skills_watcher::SkillsWatcher;

use super::Session;
use super::SessionConfiguration;

mod agent_os_bootstrap;
mod auth_mcp_bootstrap;
mod beta_features;
mod hooks_bootstrap;
mod input;
mod mcp_startup;
mod network_proxy;
mod parallel_startup;
mod pipeline;
mod post_configured;
mod rollout_bootstrap;
mod services_bootstrap;
mod session_configured;
mod session_handle;
mod shell_bootstrap;
mod skills_watcher;
mod startup_notices;
mod telemetry;
mod thread_name_bootstrap;

impl Session {
    #[instrument(name = "session_init", level = "info", skip_all)]
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn new(
        session_configuration: SessionConfiguration,
        llm_runtime_catalog: LlmRuntimeCatalog,
        config: Arc<Config>,
        auth_manager: Arc<AuthManager>,
        models_manager: Arc<ModelsManager>,
        exec_policy: Arc<ExecPolicyManager>,
        tx_event: Sender<Event>,
        agent_status: watch::Sender<AgentStatus>,
        initial_history: InitialHistory,
        session_source: SessionSource,
        environment_manager: Arc<EnvironmentManager>,
        skills_manager: Arc<SkillsManager>,
        plugins_manager: Arc<PluginsManager>,
        mcp_manager: Arc<McpManager>,
        skills_watcher: Arc<SkillsWatcher>,
        agent_control: AgentControl,
        agent_os: Arc<AgentOs>,
    ) -> anyhow::Result<Arc<Self>> {
        pipeline::run(input::SessionStartupInput {
            session_configuration,
            llm_runtime_catalog,
            config,
            auth_manager,
            models_manager,
            exec_policy,
            tx_event,
            agent_status,
            initial_history,
            session_source,
            environment_manager,
            skills_manager,
            plugins_manager,
            mcp_manager,
            skills_watcher,
            agent_control,
            agent_os,
        })
        .await
    }
}

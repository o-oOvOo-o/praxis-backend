use std::sync::Arc;

use async_channel::Sender;
use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
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
use crate::skills_watcher::SkillsWatcher;

use super::super::SessionConfiguration;

pub(super) struct SessionStartupInput {
    pub(super) session_configuration: SessionConfiguration,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(super) config: Arc<Config>,
    pub(super) auth_manager: Arc<AuthManager>,
    pub(super) models_manager: Arc<ModelsManager>,
    pub(super) exec_policy: Arc<ExecPolicyManager>,
    pub(super) tx_event: Sender<Event>,
    pub(super) agent_status: watch::Sender<AgentStatus>,
    pub(super) initial_history: InitialHistory,
    pub(super) session_source: SessionSource,
    pub(super) environment_manager: Arc<EnvironmentManager>,
    pub(super) skills_manager: Arc<SkillsManager>,
    pub(super) plugins_manager: Arc<PluginsManager>,
    pub(super) mcp_manager: Arc<McpManager>,
    pub(super) skills_watcher: Arc<SkillsWatcher>,
    pub(super) agent_control: AgentControl,
    pub(super) agent_os: Arc<AgentOs>,
}

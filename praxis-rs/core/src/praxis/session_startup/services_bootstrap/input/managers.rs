use std::sync::Arc;

use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;

use crate::SkillsManager;
use crate::agent::AgentControl;
use crate::agent_os::AgentOs;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::skills_watcher::SkillsWatcher;

pub(in crate::praxis::session_startup) struct ServiceManagerSet {
    pub(in crate::praxis::session_startup) auth_manager: Arc<AuthManager>,
    pub(in crate::praxis::session_startup) models_manager: Arc<ModelsManager>,
    pub(in crate::praxis::session_startup) skills_manager: Arc<SkillsManager>,
    pub(in crate::praxis::session_startup) plugins_manager: Arc<PluginsManager>,
    pub(in crate::praxis::session_startup) mcp_manager: Arc<McpManager>,
    pub(in crate::praxis::session_startup) skills_watcher: Arc<SkillsWatcher>,
    pub(in crate::praxis::session_startup) agent_control: AgentControl,
    pub(in crate::praxis::session_startup) agent_os: Arc<AgentOs>,
    pub(in crate::praxis::session_startup) environment_manager: Arc<EnvironmentManager>,
}

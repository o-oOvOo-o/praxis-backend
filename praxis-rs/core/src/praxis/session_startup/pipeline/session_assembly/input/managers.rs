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

pub(in crate::praxis::session_startup::pipeline) struct SessionAssemblyManagers<'a> {
    pub(in crate::praxis::session_startup::pipeline) auth_manager: &'a Arc<AuthManager>,
    pub(in crate::praxis::session_startup::pipeline) models_manager: &'a Arc<ModelsManager>,
    pub(in crate::praxis::session_startup::pipeline) skills_manager: Arc<SkillsManager>,
    pub(in crate::praxis::session_startup::pipeline) plugins_manager: &'a Arc<PluginsManager>,
    pub(in crate::praxis::session_startup::pipeline) mcp_manager: &'a Arc<McpManager>,
    pub(in crate::praxis::session_startup::pipeline) skills_watcher: Arc<SkillsWatcher>,
    pub(in crate::praxis::session_startup::pipeline) agent_control: AgentControl,
    pub(in crate::praxis::session_startup::pipeline) agent_os: Arc<AgentOs>,
    pub(in crate::praxis::session_startup::pipeline) environment_manager: Arc<EnvironmentManager>,
}

use std::sync::Arc;

use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;

use crate::SkillsManager;
use crate::config::Config;
use crate::exec_policy::ExecPolicyManager;
use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::skills_watcher::SkillsWatcher;

pub(in crate::praxis::session_startup::pipeline::flow) struct SessionStartupServices {
    pub(in crate::praxis::session_startup::pipeline::flow) config: Arc<Config>,
    pub(in crate::praxis::session_startup::pipeline::flow) auth_manager: Arc<AuthManager>,
    pub(in crate::praxis::session_startup::pipeline::flow) models_manager: Arc<ModelsManager>,
    pub(in crate::praxis::session_startup::pipeline::flow) exec_policy: Arc<ExecPolicyManager>,
    pub(in crate::praxis::session_startup::pipeline::flow) environment_manager:
        Arc<EnvironmentManager>,
    pub(in crate::praxis::session_startup::pipeline::flow) skills_manager: Arc<SkillsManager>,
    pub(in crate::praxis::session_startup::pipeline::flow) plugins_manager: Arc<PluginsManager>,
    pub(in crate::praxis::session_startup::pipeline::flow) mcp_manager: Arc<McpManager>,
    pub(in crate::praxis::session_startup::pipeline::flow) skills_watcher: Arc<SkillsWatcher>,
}

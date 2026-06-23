use std::sync::Arc;

use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_protocol::protocol::SessionSource;
use tokio::sync::broadcast;

use crate::agent_os::AgentOs;
use crate::models_manager::manager::ModelsManager;

use super::super::THREAD_CREATED_CHANNEL_CAPACITY;
use super::super::ThreadManagerInner;
use super::super::bootstrap::should_use_test_thread_manager_behavior;
use super::super::registry::ThreadRegistry;
use super::super::services::ThreadManagerServices;

pub(super) struct ThreadManagerInnerAssembly {
    pub(super) auth_manager: Arc<AuthManager>,
    pub(super) models_manager: Arc<ModelsManager>,
    pub(super) environment_manager: Arc<EnvironmentManager>,
    pub(super) services: ThreadManagerServices,
    pub(super) session_source: SessionSource,
}

pub(super) fn assemble_thread_manager_inner(
    assembly: ThreadManagerInnerAssembly,
) -> ThreadManagerInner {
    let ThreadManagerServices {
        skills_manager,
        plugins_manager,
        mcp_manager,
        skills_watcher,
    } = assembly.services;
    let (thread_created_tx, _) = broadcast::channel(THREAD_CREATED_CHANNEL_CAPACITY);
    ThreadManagerInner {
        threads: ThreadRegistry::default(),
        thread_created_tx,
        auth_manager: assembly.auth_manager,
        models_manager: assembly.models_manager,
        environment_manager: assembly.environment_manager,
        skills_manager,
        plugins_manager,
        mcp_manager,
        skills_watcher,
        agent_os: AgentOs::new(),
        session_source: assembly.session_source,
        ops_log: should_use_test_thread_manager_behavior()
            .then(|| Arc::new(std::sync::Mutex::new(Vec::new()))),
    }
}

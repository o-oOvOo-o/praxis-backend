use std::path::PathBuf;
use std::sync::Arc;

use praxis_exec_server::EnvironmentManager;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::protocol::SessionSource;
use tokio::sync::broadcast;

use crate::ModelProviderInfo;
use crate::SkillsManager;
use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::mcp::McpManager;
use crate::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;

use super::THREAD_CREATED_CHANNEL_CAPACITY;
use super::ThreadManager;
use super::ThreadManagerInner;
use super::bootstrap::TempPraxisHomeGuard;
use super::bootstrap::build_skills_watcher;
use super::bootstrap::set_thread_manager_test_mode_for_tests;
use super::bootstrap::should_use_test_thread_manager_behavior;
use super::registry::ThreadRegistry;

impl ThreadManager {
    pub fn new(
        config: &Config,
        auth_manager: Arc<AuthManager>,
        session_source: SessionSource,
        collaboration_modes_config: CollaborationModesConfig,
        environment_manager: Arc<EnvironmentManager>,
    ) -> Self {
        let praxis_home = config.praxis_home.clone();
        let restriction_product = session_source.restriction_product();
        let (thread_created_tx, _) = broadcast::channel(THREAD_CREATED_CHANNEL_CAPACITY);
        let plugins_manager = Arc::new(PluginsManager::new_with_restriction_product(
            praxis_home.clone(),
            restriction_product,
        ));
        let mcp_manager = Arc::new(McpManager::new(Arc::clone(&plugins_manager)));
        let skills_manager = Arc::new(SkillsManager::new_with_restriction_product(
            praxis_home.clone(),
            config.bundled_skills_enabled(),
            restriction_product,
        ));
        let skills_watcher = build_skills_watcher(Arc::clone(&skills_manager));
        Self {
            state: Arc::new(ThreadManagerInner {
                threads: ThreadRegistry::default(),
                thread_created_tx,
                models_manager: Arc::new(ModelsManager::new_with_provider(
                    praxis_home,
                    auth_manager.clone(),
                    config.model_catalog.clone(),
                    collaboration_modes_config,
                    config.model_provider.clone(),
                )),
                environment_manager,
                skills_manager,
                plugins_manager,
                mcp_manager,
                skills_watcher,
                agent_os: AgentOs::new(),
                auth_manager,
                session_source,
                ops_log: should_use_test_thread_manager_behavior()
                    .then(|| Arc::new(std::sync::Mutex::new(Vec::new()))),
            }),
            _test_praxis_home_guard: None,
        }
    }

    /// Construct with a dummy AuthManager containing the provided OpenAiAccountAuth.
    /// Used for integration tests: should not be used by ordinary business logic.
    pub(crate) fn with_models_provider_for_tests(
        auth: OpenAiAccountAuth,
        provider: ModelProviderInfo,
    ) -> Self {
        set_thread_manager_test_mode_for_tests(/*enabled*/ true);
        let praxis_home = std::env::temp_dir().join(format!(
            "praxis-thread-manager-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&praxis_home)
            .unwrap_or_else(|err| panic!("temp praxis home dir create failed: {err}"));
        let mut manager = Self::with_models_provider_and_home_for_tests(
            auth,
            provider,
            praxis_home.clone(),
            Arc::new(EnvironmentManager::new(/*exec_server_url*/ None)),
        );
        manager._test_praxis_home_guard = Some(TempPraxisHomeGuard { path: praxis_home });
        manager
    }

    /// Construct with a dummy AuthManager containing the provided OpenAiAccountAuth and praxis home.
    /// Used for integration tests: should not be used by ordinary business logic.
    pub(crate) fn with_models_provider_and_home_for_tests(
        auth: OpenAiAccountAuth,
        provider: ModelProviderInfo,
        praxis_home: PathBuf,
        environment_manager: Arc<EnvironmentManager>,
    ) -> Self {
        set_thread_manager_test_mode_for_tests(/*enabled*/ true);
        let auth_manager = AuthManager::from_auth_for_testing(auth);
        let (thread_created_tx, _) = broadcast::channel(THREAD_CREATED_CHANNEL_CAPACITY);
        let restriction_product = SessionSource::Exec.restriction_product();
        let plugins_manager = Arc::new(PluginsManager::new_with_restriction_product(
            praxis_home.clone(),
            restriction_product,
        ));
        let mcp_manager = Arc::new(McpManager::new(Arc::clone(&plugins_manager)));
        let skills_manager = Arc::new(SkillsManager::new_with_restriction_product(
            praxis_home.clone(),
            /*bundled_skills_enabled*/ true,
            restriction_product,
        ));
        let skills_watcher = build_skills_watcher(Arc::clone(&skills_manager));
        Self {
            state: Arc::new(ThreadManagerInner {
                threads: ThreadRegistry::default(),
                thread_created_tx,
                models_manager: Arc::new(ModelsManager::with_provider_for_tests(
                    praxis_home,
                    auth_manager.clone(),
                    provider,
                )),
                environment_manager,
                skills_manager,
                plugins_manager,
                mcp_manager,
                skills_watcher,
                agent_os: AgentOs::new(),
                auth_manager,
                session_source: SessionSource::Exec,
                ops_log: should_use_test_thread_manager_behavior()
                    .then(|| Arc::new(std::sync::Mutex::new(Vec::new()))),
            }),
            _test_praxis_home_guard: None,
        }
    }
}

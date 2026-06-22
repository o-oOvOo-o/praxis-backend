use std::sync::Arc;

use praxis_analytics::AnalyticsEventsClient;
use praxis_features::Feature;
use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::client::ModelRuntimeRegistry;
use crate::state::SessionServices;
use crate::tools::sandboxing::ApprovalStore;

use super::super::beta_features;
use super::input::ServicesBootstrapInput;

pub(super) async fn build(input: ServicesBootstrapInput) -> anyhow::Result<SessionServices> {
    let config = input.config;
    Ok(SessionServices {
        mcp_connection_manager: Arc::new(RwLock::new(McpConnectionManager::new_uninitialized(
            &config.permissions.approval_policy,
        ))),
        mcp_startup_cancellation_token: Mutex::new(CancellationToken::new()),
        unified_exec_manager: input.unified_exec_manager,
        shell_zsh_path: config.zsh_path.clone(),
        main_execve_wrapper_exe: config.main_execve_wrapper_exe.clone(),
        analytics_events_client: AnalyticsEventsClient::new(
            Arc::clone(&input.auth_manager),
            config.chatgpt_base_url.trim_end_matches('/').to_string(),
            config.analytics_enabled,
        ),
        hooks: input.hooks,
        rollout: Mutex::new(input.rollout_recorder),
        user_shell: Arc::new(input.default_shell),
        shell_snapshot_tx: input.shell_snapshot_tx,
        show_raw_agent_reasoning: config.show_raw_agent_reasoning,
        exec_policy: input.exec_policy,
        auth_manager: Arc::clone(&input.auth_manager),
        session_telemetry: input.session_telemetry,
        models_manager: Arc::clone(&input.models_manager),
        tool_approvals: Mutex::new(ApprovalStore::default()),
        skills_manager: input.skills_manager,
        plugins_manager: Arc::clone(&input.plugins_manager),
        mcp_manager: Arc::clone(&input.mcp_manager),
        skills_watcher: input.skills_watcher,
        agent_control: input.agent_control,
        agent_os: input.agent_os,
        network_proxy: input.started_network_proxy,
        network_approval: Arc::clone(&input.network_approval),
        state_db: input.state_db_ctx,
        model_runtime: ModelRuntimeRegistry::new(
            Some(Arc::clone(&input.auth_manager)),
            input.conversation_id,
            input.session_configuration.session_source.clone(),
            config.model_verbosity,
            config.features.enabled(Feature::EnableRequestCompression),
            config.features.enabled(Feature::RuntimeMetrics),
            beta_features::model_client_beta_features_header(config.as_ref()),
        ),
        code_mode_service: crate::tools::code_mode::CodeModeService::new(),
        environment: input.environment_manager.current().await?,
    })
}

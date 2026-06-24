use std::sync::Arc;

use tokio::sync::Mutex;

use crate::state::SessionServices;
use crate::tools::sandboxing::ApprovalStore;

use super::input::ServicesBootstrapInput;

mod analytics;
mod environment;
mod mcp_runtime;
mod model_runtime;

pub(super) async fn build(input: ServicesBootstrapInput) -> anyhow::Result<SessionServices> {
    let session = input.session;
    let managers = input.managers;
    let runtime = input.runtime;
    let config = session.config;
    let mcp_runtime::McpRuntimeServices {
        connection_manager: mcp_connection_manager,
        startup_cancellation_token: mcp_startup_cancellation_token,
    } = mcp_runtime::build(config.as_ref());
    Ok(SessionServices {
        mcp_connection_manager,
        mcp_startup_cancellation_token,
        unified_exec_manager: runtime.unified_exec_manager,
        shell_zsh_path: config.zsh_path.clone(),
        main_execve_wrapper_exe: config.main_execve_wrapper_exe.clone(),
        analytics_events_client: analytics::build(config.as_ref(), &managers.auth_manager),
        hooks: runtime.hooks,
        rollout: Mutex::new(runtime.rollout_recorder),
        user_shell: Arc::new(runtime.default_shell),
        shell_snapshot_tx: runtime.shell_snapshot_tx,
        exec_policy: runtime.exec_policy,
        auth_manager: Arc::clone(&managers.auth_manager),
        session_telemetry: runtime.session_telemetry,
        models_manager: Arc::clone(&managers.models_manager),
        tool_approvals: Mutex::new(ApprovalStore::default()),
        skills_manager: managers.skills_manager,
        plugins_manager: Arc::clone(&managers.plugins_manager),
        mcp_manager: Arc::clone(&managers.mcp_manager),
        skills_watcher: managers.skills_watcher,
        agent_control: managers.agent_control,
        agent_os: managers.agent_os,
        network_proxy: runtime.started_network_proxy,
        network_approval: Arc::clone(&runtime.network_approval),
        state_db: runtime.state_db_ctx,
        model_runtime: model_runtime::build(
            config.as_ref(),
            &managers.auth_manager,
            session.conversation_id,
            &session.session_configuration,
        ),
        code_mode_service: crate::tools::code_mode::CodeModeService::new(),
        environment: environment::current(&managers.environment_manager).await?,
    })
}

mod cancellation;
mod input;
mod required_wait;
mod sandbox_state;
mod server_scope;

use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use praxis_mcp::mcp_connection_manager::praxis_apps_tools_cache_key;
use tracing::Instrument;
use tracing::info_span;

use crate::praxis::INITIAL_SUBMIT_ID;
pub(super) use input::McpStartupInput;

pub(super) async fn start(input: McpStartupInput<'_>) -> anyhow::Result<()> {
    let McpStartupInput {
        session,
        config,
        session_configuration,
        mcp_manager,
        tx_event,
        auth,
        mcp_servers,
        auth_statuses,
    } = input;

    let sandbox_state = sandbox_state::build(config, session_configuration);
    let server_scope = server_scope::McpStartupServerScope::from_servers(&mcp_servers);
    let tool_plugin_provenance = mcp_manager.tool_plugin_provenance(config);

    cancellation::reset_startup_token(session).await;
    let (mcp_connection_manager, cancel_token) = McpConnectionManager::new(
        &mcp_servers,
        config.mcp_oauth_credentials_store_mode,
        auth_statuses,
        &session_configuration.approval_policy,
        INITIAL_SUBMIT_ID.to_owned(),
        tx_event,
        sandbox_state,
        config.praxis_home.clone(),
        praxis_apps_tools_cache_key(auth),
        tool_plugin_provenance,
    )
    .instrument(info_span!(
        "session_init.mcp_manager_init",
        otel.name = "session_init.mcp_manager_init",
        session_init.enabled_mcp_server_count = server_scope.enabled_count,
        session_init.required_mcp_server_count = server_scope.required_count,
    ))
    .await;
    cancellation::install_connection_manager(session, mcp_connection_manager, cancel_token).await;
    required_wait::wait(
        session,
        &server_scope.required_servers,
        server_scope.required_count,
    )
    .await
}

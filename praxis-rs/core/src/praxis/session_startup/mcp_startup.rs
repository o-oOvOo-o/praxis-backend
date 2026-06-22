mod cancellation;
mod required_wait;
mod sandbox_state;
mod server_scope;

use std::collections::HashMap;
use std::sync::Arc;

use async_channel::Sender;
use praxis_config::types::McpServerConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use praxis_mcp::mcp_connection_manager::praxis_apps_tools_cache_key;
use praxis_protocol::protocol::Event;
use tracing::Instrument;
use tracing::info_span;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::praxis::INITIAL_SUBMIT_ID;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;

pub(super) async fn start(
    session: &Arc<Session>,
    config: &Config,
    session_configuration: &SessionConfiguration,
    mcp_manager: &McpManager,
    tx_event: Sender<Event>,
    auth: Option<&OpenAiAccountAuth>,
    mcp_servers: HashMap<String, McpServerConfig>,
    auth_statuses: HashMap<String, McpAuthStatusEntry>,
) -> anyhow::Result<()> {
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

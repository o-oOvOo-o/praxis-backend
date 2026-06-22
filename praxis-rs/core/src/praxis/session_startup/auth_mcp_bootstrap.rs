use std::collections::HashMap;
use std::sync::Arc;

use praxis_config::types::McpServerConfig;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_mcp::mcp::auth::compute_auth_statuses;

use crate::config::Config;
use crate::mcp::McpManager;

pub(super) struct AuthMcpBootstrap {
    pub(super) auth: Option<OpenAiAccountAuth>,
    pub(super) mcp_servers: HashMap<String, McpServerConfig>,
    pub(super) auth_statuses: HashMap<String, McpAuthStatusEntry>,
}

pub(super) async fn load(
    auth_manager: Arc<AuthManager>,
    config: Arc<Config>,
    mcp_manager: Arc<McpManager>,
) -> AuthMcpBootstrap {
    let auth = auth_manager.auth().await;
    let mcp_servers = mcp_manager.effective_servers(&config, auth.as_ref());
    let auth_statuses =
        compute_auth_statuses(mcp_servers.iter(), config.mcp_oauth_credentials_store_mode).await;

    AuthMcpBootstrap {
        auth,
        mcp_servers,
        auth_statuses,
    }
}

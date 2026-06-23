use std::sync::Arc;

use praxis_login::AuthManager;
use tracing::Instrument;
use tracing::info_span;

use crate::config::Config;
use crate::mcp::McpManager;

use super::super::auth_mcp_bootstrap;

pub(super) async fn load(
    auth_manager: Arc<AuthManager>,
    config: Arc<Config>,
    mcp_manager: Arc<McpManager>,
) -> auth_mcp_bootstrap::AuthMcpBootstrap {
    auth_mcp_bootstrap::load(auth_manager, config, mcp_manager)
        .instrument(info_span!(
            "session_init.auth_mcp",
            otel.name = "session_init.auth_mcp",
        ))
        .await
}

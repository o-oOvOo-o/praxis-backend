use std::sync::Arc;

use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;

pub(super) async fn reset_startup_token(session: &Arc<Session>) {
    let mut cancel_guard = session.services.mcp_startup_cancellation_token.lock().await;
    cancel_guard.cancel();
    *cancel_guard = CancellationToken::new();
}

pub(super) async fn install_connection_manager(
    session: &Arc<Session>,
    mcp_connection_manager: McpConnectionManager,
    cancel_token: CancellationToken,
) {
    {
        let mut manager_guard = session.services.mcp_connection_manager.write().await;
        *manager_guard = mcp_connection_manager;
    }
    {
        let mut cancel_guard = session.services.mcp_startup_cancellation_token.lock().await;
        if cancel_guard.is_cancelled() {
            cancel_token.cancel();
        }
        *cancel_guard = cancel_token;
    }
}

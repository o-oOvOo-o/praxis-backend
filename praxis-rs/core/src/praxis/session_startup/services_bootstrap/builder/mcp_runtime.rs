use std::sync::Arc;

use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::config::Config;

pub(super) struct McpRuntimeServices {
    pub(super) connection_manager: Arc<RwLock<McpConnectionManager>>,
    pub(super) startup_cancellation_token: Mutex<CancellationToken>,
}

pub(super) fn build(config: &Config) -> McpRuntimeServices {
    McpRuntimeServices {
        connection_manager: Arc::new(RwLock::new(McpConnectionManager::new_uninitialized(
            &config.permissions.approval_policy,
        ))),
        startup_cancellation_token: Mutex::new(CancellationToken::new()),
    }
}

use std::collections::HashMap;

use praxis_async_utils::OrCancelExt;
use praxis_mcp::mcp_connection_manager::ToolInfo;
use tokio_util::sync::CancellationToken;

use crate::error::Result as PraxisResult;
use crate::praxis::Session;

pub(super) struct McpToolSnapshot {
    pub(super) has_mcp_servers: bool,
    pub(super) tools: HashMap<String, ToolInfo>,
}

pub(super) async fn load(
    sess: &Session,
    cancellation_token: &CancellationToken,
) -> PraxisResult<McpToolSnapshot> {
    let mcp_connection_manager = sess.services.mcp_connection_manager.read().await;
    let has_mcp_servers = mcp_connection_manager.has_servers();
    let tools = mcp_connection_manager
        .list_all_tools()
        .or_cancel(cancellation_token)
        .await?;

    Ok(McpToolSnapshot {
        has_mcp_servers,
        tools,
    })
}

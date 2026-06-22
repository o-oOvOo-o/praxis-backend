use praxis_protocol::protocol::McpServerRefreshConfig;
use praxis_protocol::protocol::Op;
use tracing::warn;

use super::ThreadManager;

impl ThreadManager {
    pub async fn refresh_mcp_servers(&self, refresh_config: McpServerRefreshConfig) {
        let threads = self.state.threads.snapshot_threads().await;
        for thread in threads {
            if let Err(err) = thread
                .submit(Op::RefreshMcpServers {
                    config: refresh_config.clone(),
                })
                .await
            {
                warn!("failed to request MCP server refresh: {err}");
            }
        }
    }
}

use std::sync::Arc;

use praxis_mcp::mcp::auth::compute_auth_statuses;
use praxis_mcp::mcp::collect_mcp_snapshot_from_manager;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;

use crate::config::Config;
use crate::praxis::Session;

impl Session {
    pub(in crate::praxis) async fn list_mcp_tools(&self, config: &Arc<Config>, sub_id: String) {
        let mcp_connection_manager = self.services.mcp_connection_manager.read().await;
        let auth = self.services.auth_manager.auth().await;
        let mcp_servers = self
            .services
            .mcp_manager
            .effective_servers(config, auth.as_ref());
        let snapshot = collect_mcp_snapshot_from_manager(
            &mcp_connection_manager,
            compute_auth_statuses(mcp_servers.iter(), config.mcp_oauth_credentials_store_mode)
                .await,
        )
        .await;
        let event = Event {
            id: sub_id,
            msg: EventMsg::McpListToolsResponse(snapshot),
        };
        self.send_event_raw(event).await;
    }
}

use serde_json::Value;
use std::sync::Arc;
use tracing::warn;

use praxis_rmcp_client::ElicitationResponse;
use rmcp::model::RequestId;

use crate::praxis::Session;

use super::conversion::request_id_from_protocol;
use super::conversion::response_from_protocol;
use super::pending::remove_pending_elicitation;

impl Session {
    pub(crate) async fn apply_elicitation_response(
        self: &Arc<Self>,
        server_name: String,
        request_id: praxis_protocol::mcp::RequestId,
        decision: praxis_protocol::approvals::ElicitationAction,
        content: Option<Value>,
        meta: Option<Value>,
    ) {
        let response = response_from_protocol(decision, content, meta);
        let request_id = request_id_from_protocol(request_id);
        if let Err(err) = self
            .resolve_elicitation(server_name, request_id, response)
            .await
        {
            warn!(
                error = %err,
                "failed to resolve elicitation request in session"
            );
        }
    }

    pub async fn resolve_elicitation(
        &self,
        server_name: String,
        id: RequestId,
        response: ElicitationResponse,
    ) -> anyhow::Result<()> {
        if let Some(tx_response) = remove_pending_elicitation(self, &server_name, &id).await {
            tx_response
                .send(response)
                .map_err(|e| anyhow::anyhow!("failed to send elicitation response: {e:?}"))?;
            return Ok(());
        }

        self.services
            .mcp_connection_manager
            .read()
            .await
            .resolve_elicitation(server_name, id, response)
            .await
    }
}

use praxis_protocol::approvals::ElicitationRequestEvent;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequestParams;
use praxis_protocol::protocol::EventMsg;
use praxis_rmcp_client::ElicitationResponse;
use rmcp::model::RequestId;
use tokio::sync::oneshot;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::conversion::request_id_to_protocol;
use super::conversion::request_to_protocol;
use super::pending::insert_pending_elicitation;

impl Session {
    pub async fn request_mcp_server_elicitation(
        &self,
        turn_context: &TurnContext,
        request_id: RequestId,
        params: McpServerElicitationRequestParams,
    ) -> Option<ElicitationResponse> {
        let server_name = params.server_name.clone();
        let request = request_to_protocol(params.request, &server_name, &request_id)?;

        let (tx_response, rx_response) = oneshot::channel();
        insert_pending_elicitation(self, server_name.clone(), request_id.clone(), tx_response)
            .await;

        let id = request_id_to_protocol(request_id);
        let event = EventMsg::ElicitationRequest(ElicitationRequestEvent {
            turn_id: params.turn_id,
            server_name,
            id,
            request,
        });
        self.send_event(turn_context, event).await;
        rx_response.await.ok()
    }
}

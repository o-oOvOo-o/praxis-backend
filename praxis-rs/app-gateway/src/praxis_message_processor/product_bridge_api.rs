use tokio::sync::oneshot;

use super::*;
use crate::outgoing_message::ClientRequestResult;
use praxis_app_gateway_protocol::CUNNING3D_BRIDGE_EXTENSION_ID;
use praxis_app_gateway_protocol::Cunning3dBridgeCallParams;
use praxis_app_gateway_protocol::GatewayCapabilityKind;
use praxis_app_gateway_protocol::ServerRequestPayload;

impl PraxisMessageProcessor {
    pub(crate) async fn send_cunning3d_bridge_call(
        &self,
        thread_id: ThreadId,
        params: Cunning3dBridgeCallParams,
    ) -> Result<(RequestId, oneshot::Receiver<ClientRequestResult>), JSONRPCErrorError> {
        let connection_ids = self
            .thread_state_manager
            .subscribed_connection_ids_with_host_capability(
                thread_id,
                CUNNING3D_BRIDGE_EXTENSION_ID,
                GatewayCapabilityKind::ProductBridge,
            )
            .await;

        let Some(connection_id) = connection_ids.first().copied() else {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "No attached Cunning3D bridge host is subscribed to this thread."
                    .to_owned(),
                data: None,
            });
        };

        let sender = ThreadScopedOutgoingMessageSender::new(
            self.outgoing.clone(),
            vec![connection_id],
            thread_id,
        );
        Ok(sender
            .send_request(ServerRequestPayload::Cunning3dBridgeCall(params))
            .await)
    }
}

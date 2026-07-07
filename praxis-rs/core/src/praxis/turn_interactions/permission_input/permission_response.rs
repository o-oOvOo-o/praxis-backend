use praxis_protocol::request_permissions::PermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionsResponse;
use tokio::sync::oneshot;
use tracing::warn;

use crate::praxis::Session;

impl Session {
    pub async fn notify_request_permissions_response(
        &self,
        call_id: &str,
        response: RequestPermissionsResponse,
    ) {
        let entry = self
            .remove_pending_request_permissions_and_record_grants(call_id, &response)
            .await;

        match entry {
            Some(tx_response) => {
                tx_response.send(response).ok();
            }
            None => {
                warn!("No pending request_permissions found for call_id: {call_id}");
            }
        }
    }

    async fn remove_pending_request_permissions_and_record_grants(
        &self,
        call_id: &str,
        response: &RequestPermissionsResponse,
    ) -> Option<oneshot::Sender<RequestPermissionsResponse>> {
        let entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    let entry = ts.remove_pending_request_permissions(call_id);
                    if entry.is_some() && !response.permissions.is_empty() {
                        match response.scope {
                            PermissionGrantScope::Turn => {
                                self.grant_turn_permissions(response.permissions.clone().into());
                            }
                            PermissionGrantScope::Session => {
                                self.grant_session_permissions(response.permissions.clone().into());
                            }
                        }
                    }
                    entry
                }
                None => None,
            }
        };
        entry
    }
}

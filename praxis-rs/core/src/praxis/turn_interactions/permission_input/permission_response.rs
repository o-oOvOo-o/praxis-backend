use praxis_protocol::request_permissions::PermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionProfile;
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
        let (entry, granted_for_session) = self
            .remove_pending_request_permissions_and_record_turn_grants(call_id, &response)
            .await;

        if let Some(permissions) = granted_for_session {
            let mut state = self.state.lock().await;
            state.record_granted_permissions(permissions.into());
        }

        match entry {
            Some(tx_response) => {
                tx_response.send(response).ok();
            }
            None => {
                warn!("No pending request_permissions found for call_id: {call_id}");
            }
        }
    }

    async fn remove_pending_request_permissions_and_record_turn_grants(
        &self,
        call_id: &str,
        response: &RequestPermissionsResponse,
    ) -> (
        Option<oneshot::Sender<RequestPermissionsResponse>>,
        Option<RequestPermissionProfile>,
    ) {
        let mut granted_for_session = None;
        let entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    let entry = ts.remove_pending_request_permissions(call_id);
                    if entry.is_some() && !response.permissions.is_empty() {
                        match response.scope {
                            PermissionGrantScope::Turn => {
                                ts.record_granted_permissions(response.permissions.clone().into());
                            }
                            PermissionGrantScope::Session => {
                                granted_for_session = Some(response.permissions.clone());
                            }
                        }
                    }
                    entry
                }
                None => None,
            }
        };
        (entry, granted_for_session)
    }
}

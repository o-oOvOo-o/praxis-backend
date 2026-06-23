use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::request_permissions::PermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionProfile;
use praxis_protocol::request_permissions::RequestPermissionsArgs;
use praxis_protocol::request_permissions::RequestPermissionsEvent;
use praxis_protocol::request_permissions::RequestPermissionsResponse;
use tokio::sync::oneshot;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub async fn request_permissions(
        &self,
        turn_context: &TurnContext,
        call_id: String,
        args: RequestPermissionsArgs,
    ) -> Option<RequestPermissionsResponse> {
        if let Some(response) = disabled_request_permissions_response(turn_context) {
            return Some(response);
        }

        let (tx_response, rx_response) = oneshot::channel();
        let prev_entry = self
            .insert_pending_request_permissions(call_id.clone(), tx_response)
            .await;
        if prev_entry.is_some() {
            warn!("Overwriting existing pending request_permissions for call_id: {call_id}");
        }

        self.send_request_permissions_event(turn_context, call_id, args)
            .await;
        rx_response.await.ok()
    }

    async fn insert_pending_request_permissions(
        &self,
        call_id: String,
        tx_response: oneshot::Sender<RequestPermissionsResponse>,
    ) -> Option<oneshot::Sender<RequestPermissionsResponse>> {
        let mut active = self.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let mut ts = at.turn_state.lock().await;
                ts.insert_pending_request_permissions(call_id, tx_response)
            }
            None => None,
        }
    }

    async fn send_request_permissions_event(
        &self,
        turn_context: &TurnContext,
        call_id: String,
        args: RequestPermissionsArgs,
    ) {
        let event = EventMsg::RequestPermissions(RequestPermissionsEvent {
            call_id,
            turn_id: turn_context.sub_id.clone(),
            reason: args.reason,
            permissions: args.permissions,
        });
        self.send_event(turn_context, event).await;
    }
}

fn disabled_request_permissions_response(
    turn_context: &TurnContext,
) -> Option<RequestPermissionsResponse> {
    match turn_context.effective_approval_policy() {
        AskForApproval::Never => Some(default_turn_permissions_response()),
        AskForApproval::Granular(granular_config)
            if !granular_config.allows_request_permissions() =>
        {
            Some(default_turn_permissions_response())
        }
        AskForApproval::OnFailure
        | AskForApproval::OnRequest
        | AskForApproval::UnlessTrusted
        | AskForApproval::Granular(_) => None,
    }
}

fn default_turn_permissions_response() -> RequestPermissionsResponse {
    RequestPermissionsResponse {
        permissions: RequestPermissionProfile::default(),
        scope: PermissionGrantScope::Turn,
    }
}

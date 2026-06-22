use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::request_permissions::PermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionProfile;
use praxis_protocol::request_permissions::RequestPermissionsArgs;
use praxis_protocol::request_permissions::RequestPermissionsEvent;
use praxis_protocol::request_permissions::RequestPermissionsResponse;
use praxis_protocol::request_user_input::RequestUserInputArgs;
use praxis_protocol::request_user_input::RequestUserInputEvent;
use praxis_protocol::request_user_input::RequestUserInputResponse;
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
        match turn_context.effective_approval_policy() {
            AskForApproval::Never => {
                return Some(RequestPermissionsResponse {
                    permissions: RequestPermissionProfile::default(),
                    scope: PermissionGrantScope::Turn,
                });
            }
            AskForApproval::Granular(granular_config)
                if !granular_config.allows_request_permissions() =>
            {
                return Some(RequestPermissionsResponse {
                    permissions: RequestPermissionProfile::default(),
                    scope: PermissionGrantScope::Turn,
                });
            }
            AskForApproval::OnFailure
            | AskForApproval::OnRequest
            | AskForApproval::UnlessTrusted
            | AskForApproval::Granular(_) => {}
        }

        let (tx_response, rx_response) = oneshot::channel();
        let prev_entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.insert_pending_request_permissions(call_id.clone(), tx_response)
                }
                None => None,
            }
        };
        if prev_entry.is_some() {
            warn!("Overwriting existing pending request_permissions for call_id: {call_id}");
        }

        let event = EventMsg::RequestPermissions(RequestPermissionsEvent {
            call_id,
            turn_id: turn_context.sub_id.clone(),
            reason: args.reason,
            permissions: args.permissions,
        });
        self.send_event(turn_context, event).await;
        rx_response.await.ok()
    }

    pub async fn request_user_input(
        &self,
        turn_context: &TurnContext,
        call_id: String,
        args: RequestUserInputArgs,
    ) -> Option<RequestUserInputResponse> {
        let sub_id = turn_context.sub_id.clone();
        let (tx_response, rx_response) = oneshot::channel();
        let event_id = sub_id.clone();
        let prev_entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.insert_pending_user_input(sub_id, tx_response)
                }
                None => None,
            }
        };
        if prev_entry.is_some() {
            warn!("Overwriting existing pending user input for sub_id: {event_id}");
        }

        let event = EventMsg::RequestUserInput(RequestUserInputEvent {
            call_id,
            turn_id: turn_context.sub_id.clone(),
            questions: args.questions,
        });
        self.send_event(turn_context, event).await;
        rx_response.await.ok()
    }

    pub async fn notify_user_input_response(
        &self,
        sub_id: &str,
        response: RequestUserInputResponse,
    ) {
        let entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.remove_pending_user_input(sub_id)
                }
                None => None,
            }
        };
        match entry {
            Some(tx_response) => {
                tx_response.send(response).ok();
            }
            None => {
                warn!("No pending user input found for sub_id: {sub_id}");
            }
        }
    }

    pub async fn notify_request_permissions_response(
        &self,
        call_id: &str,
        response: RequestPermissionsResponse,
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
}

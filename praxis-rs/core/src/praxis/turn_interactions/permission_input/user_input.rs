use praxis_protocol::protocol::EventMsg;
use praxis_protocol::request_user_input::RequestUserInputArgs;
use praxis_protocol::request_user_input::RequestUserInputEvent;
use praxis_protocol::request_user_input::RequestUserInputResponse;
use tokio::sync::oneshot;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub async fn request_user_input(
        &self,
        turn_context: &TurnContext,
        call_id: String,
        args: RequestUserInputArgs,
    ) -> Option<RequestUserInputResponse> {
        let sub_id = turn_context.sub_id.clone();
        let event_id = sub_id.clone();
        let (tx_response, rx_response) = oneshot::channel();
        let prev_entry = self.insert_pending_user_input(sub_id, tx_response).await;
        if prev_entry.is_some() {
            warn!("Overwriting existing pending user input for sub_id: {event_id}");
        }

        self.send_request_user_input_event(turn_context, call_id, args)
            .await;
        rx_response.await.ok()
    }

    pub async fn notify_user_input_response(
        &self,
        sub_id: &str,
        response: RequestUserInputResponse,
    ) {
        let entry = self.remove_pending_user_input(sub_id).await;
        match entry {
            Some(tx_response) => {
                tx_response.send(response).ok();
            }
            None => {
                warn!("No pending user input found for sub_id: {sub_id}");
            }
        }
    }

    async fn insert_pending_user_input(
        &self,
        sub_id: String,
        tx_response: oneshot::Sender<RequestUserInputResponse>,
    ) -> Option<oneshot::Sender<RequestUserInputResponse>> {
        let mut active = self.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let mut ts = at.turn_state.lock().await;
                ts.insert_pending_user_input(sub_id, tx_response)
            }
            None => None,
        }
    }

    async fn remove_pending_user_input(
        &self,
        sub_id: &str,
    ) -> Option<oneshot::Sender<RequestUserInputResponse>> {
        let mut active = self.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let mut ts = at.turn_state.lock().await;
                ts.remove_pending_user_input(sub_id)
            }
            None => None,
        }
    }

    async fn send_request_user_input_event(
        &self,
        turn_context: &TurnContext,
        call_id: String,
        args: RequestUserInputArgs,
    ) {
        let event = EventMsg::RequestUserInput(RequestUserInputEvent {
            call_id,
            turn_id: turn_context.sub_id.clone(),
            questions: args.questions,
        });
        self.send_event(turn_context, event).await;
    }
}

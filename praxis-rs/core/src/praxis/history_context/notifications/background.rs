use praxis_protocol::protocol::BackgroundEventEvent;
use praxis_protocol::protocol::EventMsg;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(crate) async fn notify_background_event(
        &self,
        turn_context: &TurnContext,
        message: impl Into<String>,
    ) {
        let event = EventMsg::BackgroundEvent(BackgroundEventEvent {
            message: message.into(),
        });
        self.send_event(turn_context, event).await;
    }
}

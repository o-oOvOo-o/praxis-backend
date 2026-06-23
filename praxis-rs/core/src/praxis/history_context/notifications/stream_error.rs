use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::StreamErrorEvent;

use crate::error::PraxisErr;
use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(crate) async fn notify_stream_error(
        &self,
        turn_context: &TurnContext,
        message: impl Into<String>,
        praxis_error: PraxisErr,
    ) {
        let additional_details = praxis_error.to_string();
        let praxis_error_info = PraxisErrorInfo::ResponseStreamDisconnected {
            http_status_code: praxis_error.http_status_code_value(),
        };
        let event = EventMsg::StreamError(StreamErrorEvent {
            message: message.into(),
            praxis_error_info: Some(praxis_error_info),
            additional_details: Some(additional_details),
        });
        self.send_event(turn_context, event).await;
    }
}

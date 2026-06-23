use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RawResponseItemEvent;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(in crate::praxis::history_context::recording) async fn send_raw_response_items(
        &self,
        turn_context: &TurnContext,
        items: &[ResponseItem],
    ) {
        for item in items {
            self.send_event(
                turn_context,
                EventMsg::RawResponseItem(RawResponseItemEvent { item: item.clone() }),
            )
            .await;
        }
    }
}

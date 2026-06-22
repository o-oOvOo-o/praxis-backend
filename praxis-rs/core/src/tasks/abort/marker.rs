use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;

use crate::contextual_user_message::TURN_ABORTED_CLOSE_TAG;
use crate::contextual_user_message::TURN_ABORTED_OPEN_TAG;

const TURN_ABORTED_INTERRUPTED_GUIDANCE: &str = "The user interrupted the previous turn on purpose. Any running unified exec processes may still be running in the background. If any tools/commands were aborted, they may have partially executed.";

pub(crate) fn interrupted_turn_history_marker() -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "{TURN_ABORTED_OPEN_TAG}\n{TURN_ABORTED_INTERRUPTED_GUIDANCE}\n{TURN_ABORTED_CLOSE_TAG}"
            ),
        }],
        end_turn: None,
        phase: None,
    }
}

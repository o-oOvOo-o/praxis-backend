use std::sync::Arc;

use praxis_protocol::protocol::Op;
use praxis_protocol::user_input::UserInput;

use super::super::Session;

impl Session {
    pub(crate) async fn route_realtime_text_input(self: &Arc<Self>, text: String) {
        self.submit_user_input_or_turn(
            self.next_internal_sub_id(),
            Op::UserInput {
                items: vec![UserInput::Text {
                    text,
                    text_elements: Vec::new(),
                }],
                final_output_json_schema: None,
            },
        )
        .await;
    }
}

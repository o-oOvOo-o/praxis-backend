use std::sync::Arc;

use praxis_protocol::user_input::UserInput;

use super::super::Session;

impl Session {
    pub(crate) async fn route_realtime_text_input(self: &Arc<Self>, text: String) {
        let config = self.get_config().await;
        self.submit_user_turn(
            self.next_internal_sub_id(),
            config.user_turn_op(
                vec![UserInput::Text {
                    text,
                    text_elements: Vec::new(),
                }],
                None,
            ),
        )
        .await;
    }
}

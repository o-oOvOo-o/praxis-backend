use std::sync::Arc;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::stream_item_delta::emit_output_text_delta;
use super::StreamItemState;

impl StreamItemState {
    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_output_text_delta(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        delta: String,
    ) {
        emit_output_text_delta(
            sess,
            turn_context,
            self.active_item.as_ref(),
            self.plan_mode_state.as_mut(),
            &mut self.assistant_message_stream_parsers,
            delta,
        )
        .await;
    }
}

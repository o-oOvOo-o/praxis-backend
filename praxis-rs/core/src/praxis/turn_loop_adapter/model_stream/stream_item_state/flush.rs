use std::sync::Arc;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::assistant_text_stream::flush_assistant_text_segments_all;
use super::StreamItemState;

impl StreamItemState {
    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn flush_assistant_text(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
    ) {
        flush_assistant_text_segments_all(
            sess,
            turn_context,
            self.plan_mode_state.as_mut(),
            &mut self.assistant_message_stream_parsers,
        )
        .await;
    }
}

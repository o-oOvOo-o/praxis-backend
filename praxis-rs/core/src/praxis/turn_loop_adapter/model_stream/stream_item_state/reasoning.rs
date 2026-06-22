use std::sync::Arc;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::reasoning_delta_stream::emit_reasoning_content_delta;
use super::super::reasoning_delta_stream::emit_reasoning_summary_delta;
use super::StreamItemState;

impl StreamItemState {
    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_reasoning_summary_delta(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        delta: String,
        summary_index: i64,
    ) {
        emit_reasoning_summary_delta(
            sess,
            turn_context,
            self.active_item.as_ref(),
            delta,
            summary_index,
        )
        .await;
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_reasoning_content_delta(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        delta: String,
        content_index: i64,
    ) {
        emit_reasoning_content_delta(
            sess,
            turn_context,
            self.active_item.as_ref(),
            delta,
            content_index,
        )
        .await;
    }
}

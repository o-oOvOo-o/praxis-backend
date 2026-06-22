use std::sync::Arc;

use praxis_protocol::models::ResponseItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::stream_item_start::start_stream_item;
use super::StreamItemState;

impl StreamItemState {
    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_output_item_added(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        item: ResponseItem,
    ) {
        if let Some(turn_item) = start_stream_item(
            sess,
            turn_context,
            item,
            self.plan_mode,
            self.plan_mode_state.as_mut(),
            &mut self.assistant_message_stream_parsers,
        )
        .await
        {
            self.active_item = Some(turn_item);
        }
    }
}

use std::sync::Arc;

use praxis_protocol::models::ResponseItem;

use crate::error::Result as PraxisResult;
use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::stream_item_completion::complete_non_tool_output_item;
use super::StreamItemState;

impl StreamItemState {
    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_completed_non_tool_output_item(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        item: ResponseItem,
    ) -> PraxisResult<Option<String>> {
        complete_non_tool_output_item(
            sess,
            turn_context,
            &mut self.active_item,
            &mut self.last_agent_message,
            self.plan_mode_state.as_mut(),
            &mut self.assistant_message_stream_parsers,
            item,
        )
        .await
    }
}

use praxis_loop::model::ModelEvent;
use praxis_loop::model::TokenUsage as LoopTokenUsage;
use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

use super::super::stream_item_state::StreamItemState;

use super::super::PraxisModelStreamInput;

pub(super) async fn finish_completed_stream(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    protocol_usage: Option<ProtocolTokenUsage>,
    loop_usage: LoopTokenUsage,
) -> ModelEvent {
    stream_items
        .flush_assistant_text(&input.session, &input.turn_context)
        .await;
    input
        .session
        .update_token_usage_info(&input.turn_context, protocol_usage.as_ref())
        .await;
    ModelEvent::Completed(loop_usage)
}

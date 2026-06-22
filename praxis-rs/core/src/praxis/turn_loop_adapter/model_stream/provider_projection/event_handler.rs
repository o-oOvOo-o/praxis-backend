use praxis_loop::outcome::LoopResult;

use super::super::stream_item_state::StreamItemState;
use super::response_event::ModelResponseEvent;
use super::response_event::classify_response_event;

use super::super::PraxisModelStreamInput;
use super::super::item_completion::handle_completed_provider_item;
use super::event::ProviderEventProjection;
use super::incremental;
use super::terminal;

pub(super) async fn handle_provider_event(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    event: crate::client_common::ResponseEvent,
) -> LoopResult<ProviderEventProjection> {
    match classify_response_event(event) {
        ModelResponseEvent::ItemAdded(item) => {
            Ok(incremental::record_item_added(input, stream_items, item).await)
        }
        ModelResponseEvent::TextDelta(delta) => {
            Ok(incremental::record_text_delta(input, stream_items, delta).await)
        }
        ModelResponseEvent::ReasoningSummaryDelta {
            delta,
            summary_index,
        } => Ok(incremental::record_reasoning_summary_delta(
            input,
            stream_items,
            delta,
            summary_index,
        )
        .await),
        ModelResponseEvent::ReasoningContentDelta {
            delta,
            content_index,
        } => Ok(incremental::record_reasoning_content_delta(
            input,
            stream_items,
            delta,
            content_index,
        )
        .await),
        ModelResponseEvent::ReasoningSummaryPartAdded { summary_index } => Ok(
            incremental::reasoning_summary_part_added(stream_items, summary_index),
        ),
        ModelResponseEvent::ItemDone(item) => {
            handle_completed_provider_item(input, stream_items, item).await
        }
        ModelResponseEvent::Completed { token_usage } => Ok(terminal::completed(token_usage)),
        ModelResponseEvent::Effect(effect) => Ok(terminal::effect(effect)),
        ModelResponseEvent::Ignore => Ok(terminal::ignore()),
    }
}

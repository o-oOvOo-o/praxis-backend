use praxis_protocol::models::ResponseItem;

use super::super::PraxisModelStreamInput;
use super::super::stream_item_state::StreamItemState;
use super::effect::ProviderEffect;
use super::event::ModelOutputObservation;
use super::event::ProviderEventProjection;

pub(super) async fn record_item_added(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    item: ResponseItem,
) -> ProviderEventProjection {
    stream_items
        .handle_output_item_added(&input.session, &input.turn_context, item)
        .await;
    ProviderEventProjection::ignore(ModelOutputObservation::Observed)
}

pub(super) async fn record_text_delta(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    delta: String,
) -> ProviderEventProjection {
    stream_items
        .handle_output_text_delta(&input.session, &input.turn_context, delta)
        .await;
    ProviderEventProjection::ignore(ModelOutputObservation::Observed)
}

pub(super) async fn record_reasoning_summary_delta(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    delta: String,
    summary_index: i64,
) -> ProviderEventProjection {
    stream_items
        .handle_reasoning_summary_delta(&input.session, &input.turn_context, delta, summary_index)
        .await;
    ProviderEventProjection::ignore(ModelOutputObservation::Observed)
}

pub(super) async fn record_reasoning_content_delta(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    delta: String,
    content_index: i64,
) -> ProviderEventProjection {
    stream_items
        .handle_reasoning_content_delta(&input.session, &input.turn_context, delta, content_index)
        .await;
    ProviderEventProjection::ignore(ModelOutputObservation::Observed)
}

pub(super) fn reasoning_summary_part_added(
    stream_items: &StreamItemState,
    summary_index: i64,
) -> ProviderEventProjection {
    ProviderEventProjection::core_effect(
        ProviderEffect::ReasoningSummaryPartAdded {
            item_id: stream_items.active_item_id(),
            summary_index,
        },
        ModelOutputObservation::Observed,
    )
}

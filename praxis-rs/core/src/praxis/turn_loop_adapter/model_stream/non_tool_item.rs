use praxis_loop::model::ModelEvent;
use praxis_loop::outcome::LoopResult;
use praxis_protocol::models::ResponseItem;

use super::stream_item_state::StreamItemState;

use super::PraxisModelStreamInput;
use super::error_bridge::model_error;
use super::provider_projection::ModelOutputObservation;
use super::provider_projection::ProviderEventProjection;
use super::response_item_identity::response_item_id;

pub(super) async fn record_completed_non_tool_item(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    item: ResponseItem,
) -> LoopResult<ProviderEventProjection> {
    let item_id = response_item_id(&item);
    let Some(message) = stream_items
        .handle_completed_non_tool_output_item(&input.session, &input.turn_context, item)
        .await
        .map_err(model_error)?
    else {
        return Ok(ProviderEventProjection::ignore(
            ModelOutputObservation::Observed,
        ));
    };
    input
        .bridge_state
        .record_agent_message(message.clone())
        .await;
    Ok(ProviderEventProjection::loop_event(
        ModelEvent::RecordedFinalText {
            item_id,
            text: message,
        },
    ))
}

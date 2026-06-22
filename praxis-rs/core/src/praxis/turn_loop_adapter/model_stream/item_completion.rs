use praxis_loop::outcome::LoopResult;
use praxis_protocol::models::ResponseItem;

use super::stream_item_state::StreamItemState;

use super::PraxisModelStreamInput;
use super::completed_tool_call;
use super::completed_tool_call::CompletedItemProjection;
use super::non_tool_item::record_completed_non_tool_item;
use super::provider_projection::ProviderEventProjection;

pub(super) async fn handle_completed_provider_item(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    item: ResponseItem,
) -> LoopResult<ProviderEventProjection> {
    match completed_tool_call::try_project_completed_tool_call(input, stream_items, &item).await? {
        CompletedItemProjection::Projected(projection) => Ok(projection),
        CompletedItemProjection::NonTool => {
            record_completed_non_tool_item(input, stream_items, item).await
        }
    }
}

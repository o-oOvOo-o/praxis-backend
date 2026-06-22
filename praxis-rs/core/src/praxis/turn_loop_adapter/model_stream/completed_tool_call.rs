use std::sync::Arc;

use praxis_loop::model::ModelEvent;
use praxis_loop::outcome::LoopResult;
use praxis_protocol::models::ResponseItem;
use tracing::warn;

use super::stream_item_state::StreamItemState;
use crate::turn_final_answer::tool_loop_guard_final_item;

use super::PraxisModelStreamInput;
use super::completed_tool_call_conversion;
use super::completed_tool_call_conversion::CompletedToolCallConversion;
use super::non_tool_item::record_completed_non_tool_item;
use super::provider_projection::ProviderEventProjection;

pub(super) enum CompletedItemProjection {
    Projected(ProviderEventProjection),
    NonTool,
}

pub(super) async fn try_project_completed_tool_call(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    item: &ResponseItem,
) -> LoopResult<CompletedItemProjection> {
    let call = match completed_tool_call_conversion::convert_completed_tool_call(input, item)
        .await?
    {
        CompletedToolCallConversion::ToolCall(call) => call,
        CompletedToolCallConversion::FollowupRequired => {
            return Ok(CompletedItemProjection::Projected(
                ProviderEventProjection::loop_event(ModelEvent::FollowupRequired),
            ));
        }
        CompletedToolCallConversion::NotToolCall => return Ok(CompletedItemProjection::NonTool),
    };

    if input
        .turn_context
        .tool_loop_guard
        .should_hide_tool(&call.name)
    {
        warn!(
            tool_name = call.name.as_str(),
            "hidden tool call suppressed after tool loop guard intervention"
        );
        let final_item =
            tool_loop_guard_final_item(Arc::clone(&input.session), call.name.as_str()).await;
        return record_completed_non_tool_item(input, stream_items, final_item)
            .await
            .map(CompletedItemProjection::Projected);
    }

    tracing::info!(
        thread_id = %input.session.conversation_id,
        "ToolCall: {} {}",
        call.name,
        call.arguments
    );
    Ok(CompletedItemProjection::Projected(
        ProviderEventProjection::loop_event(ModelEvent::ToolCall(call)),
    ))
}

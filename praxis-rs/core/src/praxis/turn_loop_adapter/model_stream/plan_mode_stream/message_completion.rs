use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_utils_stream_parser::extract_proposed_plan_text;
use praxis_utils_stream_parser::strip_citations;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::PlanModeStreamState;

pub(super) async fn maybe_complete_plan_item_from_message(
    sess: &Session,
    turn_context: &TurnContext,
    state: &mut PlanModeStreamState,
    item: &ResponseItem,
) {
    if let ResponseItem::Message { role, content, .. } = item
        && role == "assistant"
    {
        let mut text = String::new();
        for entry in content {
            if let ContentItem::OutputText { text: chunk } = entry {
                text.push_str(chunk);
            }
        }
        if let Some(plan_text) = extract_proposed_plan_text(&text) {
            let (plan_text, _citations) = strip_citations(&plan_text);
            if !state.plan_item_started() {
                state.start_plan_item(sess, turn_context).await;
            }
            state
                .complete_plan_item_with_text(sess, turn_context, plan_text)
                .await;
        }
    }
}

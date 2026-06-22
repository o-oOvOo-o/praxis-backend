use std::sync::Arc;

use praxis_protocol::items::TurnItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::plan_mode_stream::PlanModeStreamState;

pub(super) async fn emit_or_queue_started_item(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    turn_item: &TurnItem,
    plan_mode_state: Option<&mut PlanModeStreamState>,
) {
    if let Some(state) = plan_mode_state
        && matches!(turn_item, TurnItem::AgentMessage(_))
    {
        state.insert_pending_agent_message(turn_item.id(), turn_item.clone());
        return;
    }

    sess.emit_turn_item_started(turn_context, turn_item).await;
}

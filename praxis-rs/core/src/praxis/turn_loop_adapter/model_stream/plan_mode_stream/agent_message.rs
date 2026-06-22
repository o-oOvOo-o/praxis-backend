use praxis_protocol::items::AgentMessageContent;
use praxis_protocol::items::AgentMessageItem;
use praxis_protocol::items::TurnItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::PlanModeStreamState;

fn agent_message_text(item: &AgentMessageItem) -> String {
    item.content
        .iter()
        .map(|entry| match entry {
            AgentMessageContent::Text { text } => text.as_str(),
        })
        .collect()
}

async fn emit_agent_message_in_plan_mode(
    sess: &Session,
    turn_context: &TurnContext,
    agent_message: AgentMessageItem,
    state: &mut PlanModeStreamState,
) {
    let agent_message_id = agent_message.id.clone();
    let text = agent_message_text(&agent_message);
    if text.trim().is_empty() {
        state.forget_agent_message(&agent_message_id);
        return;
    }

    state
        .emit_pending_agent_message_start(sess, turn_context, &agent_message_id)
        .await;

    if !state.agent_message_started(&agent_message_id) {
        let start_item = state
            .take_pending_agent_message(&agent_message_id)
            .unwrap_or_else(|| {
                TurnItem::AgentMessage(AgentMessageItem {
                    id: agent_message_id.clone(),
                    content: Vec::new(),
                    phase: None,
                    memory_citation: None,
                })
            });
        sess.emit_turn_item_started(turn_context, &start_item).await;
        state.mark_agent_message_started(agent_message_id.clone());
    }

    sess.emit_turn_item_completed(turn_context, TurnItem::AgentMessage(agent_message))
        .await;
    state.clear_agent_message_started(&agent_message_id);
}

pub(super) async fn emit_turn_item_in_plan_mode(
    sess: &Session,
    turn_context: &TurnContext,
    turn_item: TurnItem,
    previously_active_item: Option<&TurnItem>,
    state: &mut PlanModeStreamState,
) {
    match turn_item {
        TurnItem::AgentMessage(agent_message) => {
            emit_agent_message_in_plan_mode(sess, turn_context, agent_message, state).await;
        }
        _ => {
            if previously_active_item.is_none() {
                sess.emit_turn_item_started(turn_context, &turn_item).await;
            }
            sess.emit_turn_item_completed(turn_context, turn_item).await;
        }
    }
}

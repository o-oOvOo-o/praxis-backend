use std::sync::Arc;

use super::super::Session;
use super::super::TurnContext;
use super::super::turn_compaction::record_empty_model_recovery;

pub(super) async fn recover_empty_model_completion_if_needed(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    last_agent_message: &Option<String>,
) -> bool {
    if should_skip_empty_model_recovery(turn_context, last_agent_message) {
        return false;
    }

    if let Some(message) = turn_context.tool_loop_guard.record_empty_model_completion() {
        record_empty_model_recovery(session, turn_context, message).await;
        return true;
    }

    false
}

fn should_skip_empty_model_recovery(
    turn_context: &Arc<TurnContext>,
    last_agent_message: &Option<String>,
) -> bool {
    last_agent_message.is_some()
        || turn_context.tool_loop_guard.has_terminal_list_agents()
        || turn_context.tool_loop_guard.has_subagent_tool_calls()
        || turn_context.tool_loop_guard.has_terminal_model_error()
}

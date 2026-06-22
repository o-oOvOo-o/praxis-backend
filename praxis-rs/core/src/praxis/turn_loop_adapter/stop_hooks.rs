use std::sync::Arc;

use super::super::Session;
use super::super::TurnContext;

mod after_agent;
mod stop_lifecycle;

use after_agent::run_after_agent_hooks;
use stop_lifecycle::StopHookLifecycleDecision;
use stop_lifecycle::run_stop_hook_lifecycle;

pub(super) enum TurnStopHooksDecision {
    ContinueTurn,
    CompleteTurn,
    AbortTurn,
}

pub(super) async fn run_turn_completion_hooks(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    model_request_input_messages: Vec<String>,
    last_agent_message: Option<String>,
    stop_hook_active: &mut bool,
) -> TurnStopHooksDecision {
    match run_stop_hook_lifecycle(
        sess,
        turn_context,
        last_agent_message.clone(),
        stop_hook_active,
    )
    .await
    {
        StopHookLifecycleDecision::ContinueTurn => TurnStopHooksDecision::ContinueTurn,
        StopHookLifecycleDecision::CompleteTurn => TurnStopHooksDecision::CompleteTurn,
        StopHookLifecycleDecision::RunAfterAgent => {
            run_after_agent_hooks(
                sess,
                turn_context,
                model_request_input_messages,
                last_agent_message,
            )
            .await
        }
    }
}

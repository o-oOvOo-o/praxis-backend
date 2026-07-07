use std::sync::Arc;
use std::time::Instant;

use praxis_protocol::protocol::TokenUsage;
use tracing::warn;

use crate::goals::GoalRuntimeEvent;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::ActiveTurn;

pub(super) async fn mark_task_turn_started(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> TokenUsage {
    turn_context
        .turn_timing_state
        .mark_turn_started(Instant::now())
        .await;
    let token_usage_at_turn_start = session.total_token_usage().await.unwrap_or_default();
    if let Err(err) = session
        .goal_runtime_apply(GoalRuntimeEvent::TurnStarted {
            turn_context: turn_context.as_ref(),
            token_usage: token_usage_at_turn_start.clone(),
        })
        .await
    {
        warn!("failed to apply goal turn-start runtime event: {err}");
    }
    token_usage_at_turn_start
}

pub(super) async fn prepare_active_turn_state(
    session: &Arc<Session>,
    token_usage_at_turn_start: TokenUsage,
) {
    session.clear_turn_permissions();
    let pending_input = session.drain_pending_input_for_started_turn().await;
    let turn_state = {
        let mut active = session.active_turn.lock().await;
        let turn = active.get_or_insert_with(ActiveTurn::default);
        debug_assert!(turn.tasks.is_empty());
        Arc::clone(&turn.turn_state)
    };
    let mut turn_state = turn_state.lock().await;
    turn_state.token_usage_at_turn_start = token_usage_at_turn_start;
    for item in pending_input {
        turn_state.push_pending_input(item);
    }
}

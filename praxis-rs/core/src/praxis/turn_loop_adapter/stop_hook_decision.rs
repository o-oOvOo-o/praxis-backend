use std::sync::Arc;

use super::super::Session;
use super::super::TurnContext;
use super::state::PraxisTurnBridgeState;
use super::stop_hooks::TurnStopHooksDecision;
use super::stop_hooks::run_turn_completion_hooks;

pub(super) async fn run_stop_hooks(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    bridge_state: &Arc<PraxisTurnBridgeState>,
    input_messages: Vec<String>,
    last_agent_message: Option<String>,
) -> TurnStopHooksDecision {
    bridge_state
        .record_optional_agent_message(last_agent_message.clone())
        .await;
    let mut stop_hook_active = bridge_state.stop_hook_active().await;
    let decision = run_turn_completion_hooks(
        session,
        turn_context,
        input_messages,
        last_agent_message,
        &mut stop_hook_active,
    )
    .await;
    bridge_state.set_stop_hook_active(stop_hook_active).await;
    decision
}

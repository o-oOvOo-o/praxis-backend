use praxis_loop::TurnError;
use praxis_protocol::models::DeveloperInstructions;
use praxis_protocol::models::ResponseItem;

use super::super::PraxisTurnHooks;

pub(super) async fn record_pending_followup_intervention(
    hooks: &PraxisTurnHooks,
) -> Result<(), TurnError> {
    if let Some(message) = hooks
        .turn_context
        .tool_loop_guard
        .take_followup_intervention()
    {
        let intervention: ResponseItem = DeveloperInstructions::new(message).into();
        hooks
            .session
            .record_conversation_items(&hooks.turn_context, std::slice::from_ref(&intervention))
            .await;
    }
    Ok(())
}

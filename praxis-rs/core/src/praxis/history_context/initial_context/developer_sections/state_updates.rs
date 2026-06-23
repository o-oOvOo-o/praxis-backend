use crate::praxis::TurnContext;

use super::super::state_snapshot::InitialContextStateSnapshot;

pub(super) fn push_model_update(
    sections: &mut Vec<String>,
    turn_context: &TurnContext,
    snapshot: &InitialContextStateSnapshot,
) {
    if let Some(model_switch_message) =
        crate::context_manager::updates::build_model_instructions_update_item(
            snapshot.previous_turn_settings.as_ref(),
            turn_context,
        )
    {
        sections.push(model_switch_message.into_text());
    }
}

pub(super) fn push_realtime_update(
    sections: &mut Vec<String>,
    turn_context: &TurnContext,
    snapshot: &InitialContextStateSnapshot,
) {
    if let Some(realtime_update) = crate::context_manager::updates::build_initial_realtime_item(
        snapshot.reference_context_item.as_ref(),
        snapshot.previous_turn_settings.as_ref(),
        turn_context,
    ) {
        sections.push(realtime_update.into_text());
    }
}

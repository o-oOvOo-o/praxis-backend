use praxis_protocol::config_types::ModeKind;

use super::super::super::TurnContext;

pub(super) fn build_collaboration_mode(
    turn_context: &TurnContext,
) -> praxis_loop::context::CollaborationMode {
    match turn_context.collaboration_mode.mode {
        ModeKind::Plan => praxis_loop::context::CollaborationMode::ReadOnly,
        ModeKind::Default | ModeKind::PairProgramming | ModeKind::Execute => {
            praxis_loop::context::CollaborationMode::FullAccess
        }
    }
}

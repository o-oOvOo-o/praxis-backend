use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::TurnContextItem;

use crate::praxis::PreviousTurnSettings;
use crate::praxis::Session;

pub(super) struct InitialContextStateSnapshot {
    pub(super) reference_context_item: Option<TurnContextItem>,
    pub(super) previous_turn_settings: Option<PreviousTurnSettings>,
    pub(super) collaboration_mode: CollaborationMode,
    pub(super) base_instructions: String,
    pub(super) session_source: SessionSource,
}

impl InitialContextStateSnapshot {
    pub(super) async fn capture(session: &Session) -> Self {
        let state = session.state.lock().await;
        Self {
            reference_context_item: state.reference_context_item(),
            previous_turn_settings: state.previous_turn_settings(),
            collaboration_mode: state.session_configuration.collaboration_mode.clone(),
            base_instructions: state.session_configuration.base_instructions.clone(),
            session_source: state.session_configuration.session_source.clone(),
        }
    }
}

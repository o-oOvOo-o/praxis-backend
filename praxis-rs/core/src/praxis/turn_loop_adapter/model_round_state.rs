use std::sync::Arc;

use crate::client::ModelClientSession;
use crate::tools::context::SharedTurnDiffTracker;
use crate::turn_diff_tracker::TurnDiffTracker;

use super::super::Session;
use super::super::TurnContext;

pub(super) struct PraxisModelRoundState {
    turn_diff_tracker: SharedTurnDiffTracker,
    client_session: ModelClientSession,
    server_model_warning_emitted_for_turn: bool,
}

impl PraxisModelRoundState {
    pub(super) fn new(
        sess: &Session,
        turn_context: &TurnContext,
        prewarmed_client_session: Option<ModelClientSession>,
    ) -> Self {
        let client_session = match prewarmed_client_session {
            Some(client_session)
                if client_session.matches_provider(
                    &turn_context.config.model_provider_id,
                    &turn_context.provider,
                ) =>
            {
                client_session
            }
            Some(_) | None => sess.services.model_runtime.new_session_for(
                &turn_context.config.model_provider_id,
                &turn_context.provider,
            ),
        };

        Self {
            turn_diff_tracker: Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new())),
            client_session,
            server_model_warning_emitted_for_turn: false,
        }
    }

    pub(super) fn turn_diff_tracker(&self) -> SharedTurnDiffTracker {
        Arc::clone(&self.turn_diff_tracker)
    }

    pub(super) fn client_session_mut(&mut self) -> &mut ModelClientSession {
        &mut self.client_session
    }

    pub(super) fn server_model_warning_emitted_for_turn_mut(&mut self) -> &mut bool {
        &mut self.server_model_warning_emitted_for_turn
    }
}

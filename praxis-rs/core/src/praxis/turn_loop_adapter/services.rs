use super::super::Session;
use super::super::TurnContext;
use super::model_round_state::PraxisModelRoundState;
use super::state::PraxisTurnBridgeState;
use super::steering_decision;
use super::steering_decision::PraxisSteeringOutcome;
use super::tool_runtime_slot::ModelRoundToolsSlot;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::client::ModelClientSession;
use crate::tools::context::SharedTurnDiffTracker;

mod event_sink;
mod history;
mod loop_event_sink_projection;
mod model_service;
mod steering;
mod tool_access;

pub(super) struct PraxisTurnServices {
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    bridge_state: Arc<PraxisTurnBridgeState>,
    runtime_state: Arc<Mutex<PraxisModelRoundState>>,
    tool_runtime_slot: ModelRoundToolsSlot,
}

impl PraxisTurnServices {
    pub(super) fn new(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        bridge_state: Arc<PraxisTurnBridgeState>,
        prewarmed_client_session: Option<ModelClientSession>,
    ) -> Self {
        let runtime_state = PraxisModelRoundState::new(
            sess.as_ref(),
            turn_context.as_ref(),
            prewarmed_client_session,
        );
        Self {
            session: sess,
            turn_context,
            bridge_state,
            runtime_state: Arc::new(Mutex::new(runtime_state)),
            tool_runtime_slot: ModelRoundToolsSlot::default(),
        }
    }

    pub(super) fn session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }

    pub(super) fn turn_context(&self) -> Arc<TurnContext> {
        Arc::clone(&self.turn_context)
    }

    pub(super) async fn turn_diff_tracker(&self) -> SharedTurnDiffTracker {
        self.runtime_state.lock().await.turn_diff_tracker()
    }

    pub(super) async fn last_agent_message(&self) -> Option<String> {
        self.bridge_state.last_agent_message().await
    }

    pub(super) async fn process_pending_input_for_round(&self) -> PraxisSteeringOutcome {
        steering_decision::process_pending_input_for_round(&self.session, &self.turn_context).await
    }
}

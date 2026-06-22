use std::sync::Arc;

use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use super::super::Session;
use super::super::TurnContext;
use super::state::PraxisTurnBridgeState;
use praxis_loop::decisions::ContextPressureDecision;
use praxis_loop::decisions::ContextPressureView;
use praxis_loop::decisions::PrepareContextDecision;
use praxis_loop::decisions::PrepareContextView;
use praxis_loop::decisions::RoundDecision;
use praxis_loop::decisions::RoundOutcomeView;
use praxis_loop::decisions::TurnStartDecision;
use praxis_loop::decisions::TurnStopDecision;
use praxis_loop::decisions::TurnStopView;
use praxis_loop::hooks::TurnHooks;

mod context_pressure;
mod followup;
mod prepare;
mod round;
mod start;
mod stop;

pub(super) struct PraxisTurnHooks {
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
    bridge_state: Arc<PraxisTurnBridgeState>,
    cancellation_token: CancellationToken,
}

impl PraxisTurnHooks {
    pub(super) fn new(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
        bridge_state: Arc<PraxisTurnBridgeState>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            session: sess,
            turn_context,
            input,
            bridge_state,
            cancellation_token,
        }
    }
}

#[async_trait]
impl TurnHooks for PraxisTurnHooks {
    async fn on_turn_start(&self, _ctx: &praxis_loop::TurnContext) -> TurnStartDecision {
        start::on_turn_start(self).await
    }

    async fn on_context_pressure(&self, _view: ContextPressureView<'_>) -> ContextPressureDecision {
        context_pressure::on_context_pressure(self).await
    }

    async fn prepare_context(&self, _view: PrepareContextView<'_>) -> PrepareContextDecision {
        prepare::prepare_context(self).await
    }

    async fn after_model_round(&self, view: RoundOutcomeView<'_>) -> RoundDecision {
        round::after_model_round(self, view).await
    }

    async fn on_turn_stop(&self, view: TurnStopView<'_>) -> TurnStopDecision {
        stop::on_turn_stop(self, view).await
    }
}

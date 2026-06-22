use tokio_util::sync::CancellationToken;

use super::hooks::PraxisTurnHooks;
use super::services::PraxisTurnServices;

pub(in crate::praxis) struct PraxisTurnLoopBridge {
    pub(in crate::praxis::turn_loop_adapter) ctx: praxis_loop::TurnContext,
    pub(in crate::praxis::turn_loop_adapter) input: praxis_loop::TurnInput,
    pub(in crate::praxis::turn_loop_adapter) state: praxis_loop::TurnState,
    pub(in crate::praxis::turn_loop_adapter) services: PraxisTurnServices,
    pub(in crate::praxis::turn_loop_adapter) hooks: PraxisTurnHooks,
    pub(in crate::praxis::turn_loop_adapter) cancellation_token: CancellationToken,
}

pub(in crate::praxis) enum PraxisTurnLoopOutcome {
    Complete { last_agent_message: Option<String> },
    WantsFollowup { last_agent_message: Option<String> },
    Aborted { reason: PraxisTurnLoopAbort },
}

pub(in crate::praxis) struct PraxisTurnLoopAbort {
    pub(in crate::praxis) message: String,
    pub(in crate::praxis) cancelled: bool,
}

impl PraxisTurnLoopAbort {
    fn from_loop_error(error: praxis_loop::TurnError) -> Self {
        Self {
            message: error.message,
            cancelled: error.kind == praxis_loop::TurnErrorKind::Cancelled,
        }
    }
}

impl PraxisTurnLoopBridge {
    pub(in crate::praxis) async fn run(self) -> PraxisTurnLoopOutcome {
        let Self {
            ctx,
            input,
            state,
            services,
            hooks,
            cancellation_token,
        } = self;
        let result =
            praxis_loop::run_turn(ctx, state, &services, &hooks, input, cancellation_token).await;

        match result {
            praxis_loop::TurnResult::Complete { state } => PraxisTurnLoopOutcome::Complete {
                last_agent_message: state
                    .into_last_agent_message()
                    .or(services.last_agent_message().await),
            },
            praxis_loop::TurnResult::WantsFollowup { state } => {
                PraxisTurnLoopOutcome::WantsFollowup {
                    last_agent_message: state
                        .into_last_agent_message()
                        .or(services.last_agent_message().await),
                }
            }
            praxis_loop::TurnResult::Aborted { reason, .. } => PraxisTurnLoopOutcome::Aborted {
                reason: PraxisTurnLoopAbort::from_loop_error(reason),
            },
        }
    }
}

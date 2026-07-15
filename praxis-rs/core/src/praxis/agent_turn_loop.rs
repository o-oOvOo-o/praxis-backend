use std::sync::Arc;

use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use crate::client::ModelClientSession;

use super::Session;
use super::TurnContext;
use super::turn_loop_adapter::PraxisTurnLoopAbort;
use super::turn_loop_adapter::PraxisTurnLoopAdapter;
use super::turn_loop_adapter::PraxisTurnLoopOutcome;

pub(crate) struct AgentTurnLoopResult {
    pub(crate) last_agent_message: Option<String>,
    pub(crate) wants_followup: bool,
    pub(crate) aborted: bool,
}

impl AgentTurnLoopResult {
    fn complete(last_agent_message: Option<String>) -> Self {
        Self {
            last_agent_message,
            wants_followup: false,
            aborted: false,
        }
    }

    fn wants_followup(last_agent_message: Option<String>) -> Self {
        Self {
            last_agent_message,
            wants_followup: true,
            aborted: false,
        }
    }

    fn aborted() -> Self {
        Self {
            last_agent_message: None,
            wants_followup: false,
            aborted: true,
        }
    }
}

/// Runs one Praxis turn through the generic turn-loop crate and the Praxis adapter.
pub(crate) async fn agent_turn_loop(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
    prewarmed_client_session: Option<ModelClientSession>,
    cancellation_token: CancellationToken,
) -> AgentTurnLoopResult {
    let bridge = PraxisTurnLoopAdapter::build_bridge(
        Arc::clone(&sess),
        Arc::clone(&turn_context),
        &input,
        prewarmed_client_session,
        cancellation_token.clone(),
    )
    .await;
    match bridge.run().await {
        PraxisTurnLoopOutcome::Complete { last_agent_message } => {
            AgentTurnLoopResult::complete(last_agent_message)
        }
        PraxisTurnLoopOutcome::WantsFollowup { last_agent_message } => {
            AgentTurnLoopResult::wants_followup(last_agent_message)
        }
        PraxisTurnLoopOutcome::Aborted { reason } => {
            handle_loop_abort(&sess, &turn_context, reason).await;
            AgentTurnLoopResult::aborted()
        }
    }
}

async fn handle_loop_abort(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    reason: PraxisTurnLoopAbort,
) {
    if reason.cancelled {
        return;
    }
    if turn_context.tool_loop_guard.has_terminal_model_error() {
        return;
    }

    turn_context
        .tool_loop_guard
        .record_terminal_model_error(reason.message.clone());
    sess.turn_event_emitter(turn_context)
        .error(reason.message, None)
        .await;
}

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::trace_span;

mod recovery;

use super::agent_turn_loop::AgentTurnLoopResult;
use super::agent_turn_loop::agent_turn_loop;
use crate::client::ModelClientSession;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use praxis_protocol::user_input::UserInput;

pub(crate) async fn agent_task_loop(
    sess: Arc<Session>,
    ctx: Arc<TurnContext>,
    input: Vec<UserInput>,
    prewarmed_client_session: Option<ModelClientSession>,
    cancellation_token: CancellationToken,
) -> Option<String> {
    AgentTaskLoop::new(sess, ctx, cancellation_token)
        .run(input, prewarmed_client_session)
        .await
}

struct AgentTaskLoop {
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    cancellation_token: CancellationToken,
}

impl AgentTaskLoop {
    fn new(
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            session,
            turn_context,
            cancellation_token,
        }
    }

    async fn run(
        self,
        input: Vec<UserInput>,
        prewarmed_client_session: Option<ModelClientSession>,
    ) -> Option<String> {
        if !self.should_start(&input).await {
            return None;
        }

        let mut turn_result = self.run_turn(input, prewarmed_client_session).await;
        loop {
            if turn_result.aborted {
                return None;
            }

            let recovery_pending = recovery::recover_empty_model_completion_if_needed(
                &self.session,
                &self.turn_context,
                &turn_result.last_agent_message,
            )
            .await;
            if !recovery_pending
                && !turn_result.wants_followup
                && !self.has_pending_input("agent_task_loop_after_turn").await
            {
                return turn_result.last_agent_message;
            }

            turn_result = self.run_turn(Vec::new(), None).await;
        }
    }

    async fn should_start(&self, input: &[UserInput]) -> bool {
        !input.is_empty() || self.has_pending_input("agent_task_loop_empty_input").await
    }

    async fn has_pending_input(&self, reason: &'static str) -> bool {
        self.session.has_pending_input_bounded(reason).await
    }

    async fn run_turn(
        &self,
        input: Vec<UserInput>,
        prewarmed_client_session: Option<ModelClientSession>,
    ) -> AgentTurnLoopResult {
        let agent_turn_loop_span = trace_span!("agent_turn_loop");
        agent_turn_loop(
            Arc::clone(&self.session),
            Arc::clone(&self.turn_context),
            input,
            prewarmed_client_session,
            self.cancellation_token.child_token(),
        )
        .instrument(agent_turn_loop_span)
        .await
    }
}

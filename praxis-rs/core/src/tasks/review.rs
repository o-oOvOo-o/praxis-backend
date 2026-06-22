use std::sync::Arc;

use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::AgentTaskKind;

use super::AgentTask;

mod conversation;
mod events;
mod exit_mode;
mod templates;

#[derive(Clone, Copy)]
pub(crate) struct ReviewTask;

impl ReviewTask {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Session {
    pub(crate) async fn start_review_task(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
    ) {
        self.spawn_task(turn_context, input, ReviewTask).await;
    }
}

#[async_trait]
impl AgentTask for ReviewTask {
    fn kind(&self) -> AgentTaskKind {
        AgentTaskKind::Review
    }

    fn span_name(&self) -> &'static str {
        "agent_task.review"
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        let _ =
            session
                .services
                .session_telemetry
                .counter("praxis.task.review", /*inc*/ 1, &[]);

        let output = match conversation::start(
            session.clone(),
            ctx.clone(),
            input,
            cancellation_token.clone(),
        )
        .await
        {
            Some(receiver) => events::process(session.clone(), ctx.clone(), receiver).await,
            None => None,
        };
        if !cancellation_token.is_cancelled() {
            exit_mode::exit_review_mode(session, output.clone(), ctx.clone()).await;
        }
        None
    }

    async fn abort(&self, session: Arc<Session>, ctx: Arc<TurnContext>) {
        exit_mode::exit_review_mode(session, /*review_output*/ None, ctx).await;
    }
}

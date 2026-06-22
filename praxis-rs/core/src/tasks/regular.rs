use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::praxis::agent_task_loop;
use crate::session_startup_prewarm::SessionStartupPrewarmResolution;
use crate::state::AgentTaskKind;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::user_input::UserInput;

use super::AgentTask;

#[derive(Default)]
pub(crate) struct RegularAgentTask;

impl RegularAgentTask {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Session {
    pub(crate) async fn start_regular_task(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
    ) {
        self.start_task(turn_context, input, RegularAgentTask).await;
    }
}

#[async_trait]
impl AgentTask for RegularAgentTask {
    fn kind(&self) -> AgentTaskKind {
        AgentTaskKind::Regular
    }

    fn span_name(&self) -> &'static str {
        "agent_task.turn"
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        // Regular turns emit `TurnStarted` inline so first-turn lifecycle does
        // not wait on startup prewarm resolution.
        let event = EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: ctx.sub_id.clone(),
            model_context_window: ctx.model_context_window(),
            collaboration_mode_kind: ctx.collaboration_mode.mode,
        });
        session.send_event(ctx.as_ref(), event).await;
        session
            .set_server_reasoning_included(/*included*/ false)
            .await;
        let prewarmed_client_session = match session
            .consume_startup_prewarm_for_regular_turn(&cancellation_token)
            .await
        {
            SessionStartupPrewarmResolution::Cancelled => return None,
            SessionStartupPrewarmResolution::Unavailable { .. } => None,
            SessionStartupPrewarmResolution::Ready(prewarmed_client_session) => {
                Some(*prewarmed_client_session)
            }
        };
        agent_task_loop(
            session,
            ctx,
            input,
            prewarmed_client_session,
            cancellation_token,
        )
        .await
    }
}

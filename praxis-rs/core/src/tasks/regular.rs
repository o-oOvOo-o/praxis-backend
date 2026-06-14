use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::client::ModelClientSession;
use crate::praxis::TurnContext;
use crate::praxis::agent_turn_loop;
use crate::praxis::record_empty_model_recovery;
use crate::session_startup_prewarm::SessionStartupPrewarmResolution;
use crate::state::AgentTaskKind;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::user_input::UserInput;
use tracing::Instrument;
use tracing::trace_span;

use super::AgentTask;
use super::AgentTaskContext;

#[derive(Default)]
pub(crate) struct RegularAgentTask;

impl RegularAgentTask {
    pub(crate) fn new() -> Self {
        Self
    }
}

async fn agent_task_loop(
    sess: Arc<crate::praxis::Session>,
    ctx: Arc<TurnContext>,
    input: Vec<UserInput>,
    prewarmed_client_session: Option<ModelClientSession>,
    cancellation_token: CancellationToken,
) -> Option<String> {
    let agent_turn_loop_span = trace_span!("agent_turn_loop");
    let mut next_input = input;
    let mut prewarmed_client_session = prewarmed_client_session;
    loop {
        let last_agent_message = agent_turn_loop(
            Arc::clone(&sess),
            Arc::clone(&ctx),
            next_input,
            prewarmed_client_session.take(),
            cancellation_token.child_token(),
        )
        .instrument(agent_turn_loop_span.clone())
        .await;
        if last_agent_message.is_none()
            && !ctx.tool_loop_guard.has_terminal_list_agents()
            && !ctx.tool_loop_guard.has_subagent_tool_calls()
            && !ctx.tool_loop_guard.has_terminal_model_error()
            && let Some(message) = ctx.tool_loop_guard.record_empty_model_completion()
        {
            record_empty_model_recovery(&sess, &ctx, message).await;
            next_input = Vec::new();
            continue;
        }
        if !sess
            .has_pending_input_bounded("regular_agent_task_after_agent_turn_loop")
            .await
        {
            return last_agent_message;
        }
        next_input = Vec::new();
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
        session: Arc<AgentTaskContext>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        let sess = session.clone_session();
        // Regular turns emit `TurnStarted` inline so first-turn lifecycle does
        // not wait on startup prewarm resolution.
        let event = EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: ctx.sub_id.clone(),
            model_context_window: ctx.model_context_window(),
            collaboration_mode_kind: ctx.collaboration_mode.mode,
        });
        sess.send_event(ctx.as_ref(), event).await;
        sess.set_server_reasoning_included(/*included*/ false).await;
        let prewarmed_client_session = match sess
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
            sess,
            ctx,
            input,
            prewarmed_client_session,
            cancellation_token,
        )
        .await
    }
}

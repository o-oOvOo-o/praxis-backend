use std::sync::Arc;

use super::AgentTask;
use super::AgentTaskContext;
use crate::praxis::TurnContext;
use crate::state::AgentTaskKind;
use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Default)]
pub(crate) struct CompactTask;

#[async_trait]
impl AgentTask for CompactTask {
    fn kind(&self) -> AgentTaskKind {
        AgentTaskKind::Compact
    }

    fn span_name(&self) -> &'static str {
        "agent_task.compact"
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<AgentTaskContext>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        let session = session.clone_session();
        let compact_policy = crate::compact::compact_execution_policy_for_turn(&session, &ctx);
        let _ = if compact_policy
            == crate::llm::tasks::compact::CompactExecutionPolicy::RemoteResponses
        {
            let _ = session.services.session_telemetry.counter(
                "codex.task.compact",
                /*inc*/ 1,
                &[("type", compact_policy.telemetry_kind())],
            );
            crate::compact_remote::run_remote_compact_task(session.clone(), ctx).await
        } else {
            let _ = session.services.session_telemetry.counter(
                "codex.task.compact",
                /*inc*/ 1,
                &[("type", compact_policy.telemetry_kind())],
            );
            crate::compact::run_compact_task(session.clone(), ctx, input).await
        };
        None
    }
}

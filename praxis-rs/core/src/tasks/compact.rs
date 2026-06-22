use std::sync::Arc;

use super::AgentTask;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::AgentTaskKind;
use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Default)]
struct CompactTask;

impl Session {
    pub(crate) async fn start_compact_task(self: &Arc<Self>, sub_id: String) {
        let turn_context = self.new_default_turn_with_sub_id(sub_id).await;

        self.spawn_task(
            Arc::clone(&turn_context),
            vec![UserInput::Text {
                text: turn_context.compact_prompt().to_string(),
                // Compaction prompt is synthesized; no UI element ranges to preserve.
                text_elements: Vec::new(),
            }],
            CompactTask,
        )
        .await;
    }
}

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
        session: Arc<Session>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        let compact_policy = crate::compact::compact_execution_policy_for_turn(&session, &ctx);
        let _ = if compact_policy
            == crate::llm::tasks::compact::CompactExecutionPolicy::RemoteResponses
        {
            let _ = session.services.session_telemetry.counter(
                "praxis.task.compact",
                /*inc*/ 1,
                &[("type", compact_policy.telemetry_kind())],
            );
            crate::compact_remote::run_remote_compact_task(session.clone(), ctx).await
        } else {
            let _ = session.services.session_telemetry.counter(
                "praxis.task.compact",
                /*inc*/ 1,
                &[("type", compact_policy.telemetry_kind())],
            );
            crate::compact::run_compact_task(session.clone(), ctx, input).await
        };
        None
    }
}

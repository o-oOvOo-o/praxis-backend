use std::sync::Arc;

use praxis_protocol::user_input::UserInput;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::info_span;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tasks::AgentTask;

pub(super) fn spawn_task_runner(
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    input: Vec<UserInput>,
    task: Arc<dyn AgentTask>,
    span_name: &'static str,
    task_cancellation_token: CancellationToken,
    done: Arc<Notify>,
) -> JoinHandle<()> {
    let task_span = info_span!(
        "turn",
        otel.name = span_name,
        thread.id = %session.conversation_id,
        turn.id = %turn_context.sub_id,
        model = %turn_context.model_info.slug,
    );
    tokio::spawn(
        async move {
            let ctx_for_finish = Arc::clone(&turn_context);
            let last_agent_message = task
                .run(
                    Arc::clone(&session),
                    turn_context,
                    input,
                    task_cancellation_token.child_token(),
                )
                .await;
            session.flush_rollout().await;
            if !task_cancellation_token.is_cancelled() {
                session
                    .on_task_finished(Arc::clone(&ctx_for_finish), last_agent_message)
                    .await;
            }
            done.notify_waiters();
        }
        .instrument(task_span),
    )
}

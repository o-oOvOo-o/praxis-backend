use std::sync::Arc;

use praxis_otel::metrics::names::TURN_E2E_DURATION_METRIC;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::ActiveTurn;
use crate::state::AgentTaskKind;
use crate::state::RunningAgentTask;
use crate::tasks::AgentTask;

pub(super) async fn register_running_task(
    session: &Arc<Session>,
    turn_context: Arc<TurnContext>,
    done: Arc<Notify>,
    task_kind: AgentTaskKind,
    task: Arc<dyn AgentTask>,
    cancellation_token: CancellationToken,
    handle: JoinHandle<()>,
) {
    let mut active = session.active_turn.lock().await;
    let turn = active.get_or_insert_with(ActiveTurn::default);
    debug_assert!(turn.tasks.is_empty());
    let timer = turn_context
        .session_telemetry
        .start_timer(TURN_E2E_DURATION_METRIC, &[])
        .ok();
    let running_task = RunningAgentTask::new(
        done,
        task_kind,
        task,
        cancellation_token,
        handle.abort_handle(),
        Arc::clone(&turn_context),
        timer,
    );
    turn.add_task(running_task);
}

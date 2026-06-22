use std::sync::Arc;

use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::user_input::UserInput;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::registry::register_running_task;
use super::runner::spawn_task_runner;
use super::state::mark_task_turn_started;
use super::state::prepare_active_turn_state;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tasks::AgentTask;

impl Session {
    pub(crate) async fn spawn_task<T: AgentTask>(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
        task: T,
    ) {
        self.abort_all_tasks(TurnAbortReason::Replaced).await;
        self.clear_connector_selection().await;
        self.start_task(turn_context, input, task).await;
    }

    pub(in crate::tasks) async fn start_task<T: AgentTask>(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
        task: T,
    ) {
        let task: Arc<dyn AgentTask> = Arc::new(task);
        let task_kind = task.kind();
        let span_name = task.span_name();
        let token_usage_at_turn_start = mark_task_turn_started(self, &turn_context).await;
        let cancellation_token = CancellationToken::new();
        let done = Arc::new(Notify::new());

        prepare_active_turn_state(self, token_usage_at_turn_start).await;

        let handle = spawn_task_runner(
            Arc::clone(self),
            Arc::clone(&turn_context),
            input,
            Arc::clone(&task),
            span_name,
            cancellation_token.child_token(),
            Arc::clone(&done),
        );

        register_running_task(
            self,
            turn_context,
            done,
            task_kind,
            task,
            cancellation_token,
            handle,
        )
        .await;
    }
}

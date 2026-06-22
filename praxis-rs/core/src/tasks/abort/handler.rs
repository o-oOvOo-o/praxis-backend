use std::sync::Arc;
use std::time::Duration;

use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::TurnAbortReason;
use tokio::select;
use tracing::trace;
use tracing::warn;

use super::events;
use super::marker::interrupted_turn_history_marker;
use crate::goals::GoalRuntimeEvent;
use crate::praxis::Session;
use crate::state::RunningAgentTask;

const GRACEFULL_INTERRUPTION_TIMEOUT_MS: u64 = 100;

impl Session {
    pub(super) async fn handle_task_abort(
        self: &Arc<Self>,
        task: RunningAgentTask,
        reason: TurnAbortReason,
    ) {
        let sub_id = task.turn_context.sub_id.clone();
        if task.cancellation_token.is_cancelled() {
            return;
        }

        trace!(task_kind = ?task.kind, sub_id, "aborting running task");
        task.cancellation_token.cancel();
        task.turn_context
            .turn_metadata_state
            .cancel_git_enrichment_task();
        let agent_task = Arc::clone(&task.task);

        select! {
            _ = task.done.notified() => {
            },
            _ = tokio::time::sleep(Duration::from_millis(GRACEFULL_INTERRUPTION_TIMEOUT_MS)) => {
                warn!("task {sub_id} didn't complete gracefully after {}ms", GRACEFULL_INTERRUPTION_TIMEOUT_MS);
            }
        }

        task.handle.abort();

        agent_task
            .abort(Arc::clone(self), Arc::clone(&task.turn_context))
            .await;
        self.cleanup_aborted_task_resources(&task, &reason).await;

        if reason == TurnAbortReason::Interrupted {
            self.record_interrupted_turn_marker(&task).await;
        }

        events::emit_turn_aborted(self, task.turn_context.as_ref(), reason).await;
        events::complete_aborted_runtime_command(self).await;
    }

    async fn cleanup_aborted_task_resources(
        self: &Arc<Self>,
        task: &RunningAgentTask,
        reason: &TurnAbortReason,
    ) {
        self.services
            .agent_os
            .cleanup_thread_resources_after_abort(
                self.conversation_id,
                format!("turn_aborted:{reason:?}"),
            )
            .await;
        if let Err(err) = self
            .goal_runtime_apply(GoalRuntimeEvent::TaskAborted {
                turn_context: Some(task.turn_context.as_ref()),
            })
            .await
        {
            warn!("failed to apply goal task-aborted runtime event: {err}");
        }
    }

    async fn record_interrupted_turn_marker(self: &Arc<Self>, task: &RunningAgentTask) {
        let marker = interrupted_turn_history_marker();
        self.record_into_history(std::slice::from_ref(&marker), task.turn_context.as_ref())
            .await;
        self.persist_rollout_items(&[RolloutItem::ResponseItem(marker)])
            .await;
        self.flush_rollout().await;
    }
}

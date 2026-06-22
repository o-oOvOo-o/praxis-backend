use std::sync::Arc;

use super::super::metrics;
use super::events::emit_turn_complete;
use super::final_answer::ensure_final_agent_message;
use super::post_completion::run_post_completion_updates;
use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(crate) async fn on_task_finished(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        last_agent_message: Option<String>,
    ) {
        turn_context
            .turn_metadata_state
            .cancel_git_enrichment_task();

        let finished_task_state = self.take_finished_task_state(&turn_context).await;
        self.record_finished_task_pending_input(&turn_context, finished_task_state.pending_input)
            .await;

        let terminal_model_error = turn_context.tool_loop_guard.terminal_model_error_message();
        let turn_token_usage = metrics::emit_finished_turn_metrics(
            self.as_ref(),
            finished_task_state.token_usage_at_turn_start,
            finished_task_state.turn_tool_calls,
        )
        .await;

        let last_agent_message = ensure_final_agent_message(
            self,
            &turn_context,
            last_agent_message,
            terminal_model_error.is_some(),
        )
        .await;
        let last_agent_message_for_title = last_agent_message.clone();
        let last_agent_message_for_summary = last_agent_message.clone();
        let turn_completed = terminal_model_error.is_none();

        emit_turn_complete(self, &turn_context, last_agent_message, turn_completed).await;

        run_post_completion_updates(
            self,
            &turn_context,
            turn_token_usage.as_ref(),
            last_agent_message_for_title,
            last_agent_message_for_summary,
        )
        .await;

        if finished_task_state.should_schedule_pending_work {
            self.schedule_pending_work_continuation();
        }
    }
}

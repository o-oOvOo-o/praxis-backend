use std::sync::Arc;

use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::protocol::TokenUsage;

use crate::hook_runtime::record_pending_inputs;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) struct FinishedTaskState {
    pub(super) pending_input: Vec<ResponseInputItem>,
    pub(super) should_schedule_pending_work: bool,
    pub(super) token_usage_at_turn_start: Option<TokenUsage>,
    pub(super) turn_tool_calls: u64,
}

impl Session {
    pub(super) async fn take_finished_task_state(
        &self,
        turn_context: &TurnContext,
    ) -> FinishedTaskState {
        let turn_state = {
            let mut active = self.active_turn.lock().await;
            if let Some(at) = active.as_mut()
                && at.remove_task(&turn_context.sub_id)
            {
                let turn_state = Arc::clone(&at.turn_state);
                *active = None;
                Some(turn_state)
            } else {
                None
            }
        };
        let Some(turn_state) = turn_state else {
            return FinishedTaskState {
                pending_input: Vec::new(),
                should_schedule_pending_work: false,
                token_usage_at_turn_start: None,
                turn_tool_calls: 0,
            };
        };
        let mut turn_state = turn_state.lock().await;
        FinishedTaskState {
            pending_input: turn_state.take_pending_input(),
            should_schedule_pending_work: true,
            token_usage_at_turn_start: Some(turn_state.token_usage_at_turn_start.clone()),
            turn_tool_calls: turn_state.tool_calls,
        }
    }

    pub(super) async fn record_finished_task_pending_input(
        self: &Arc<Self>,
        turn_context: &Arc<TurnContext>,
        pending_input: Vec<ResponseInputItem>,
    ) {
        record_pending_inputs(self, turn_context, pending_input).await;
    }
}

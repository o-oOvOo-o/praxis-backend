use std::sync::Arc;

use praxis_protocol::protocol::TurnAbortReason;

use crate::praxis::Session;
use crate::state::ActiveTurn;

impl Session {
    pub(crate) async fn abort_all_tasks(self: &Arc<Self>, reason: TurnAbortReason) {
        if let Some(mut active_turn) = self.take_active_turn().await {
            for task in active_turn.drain_tasks() {
                self.handle_task_abort(task, reason.clone()).await;
            }
            active_turn.clear_pending().await;
        }
        if reason == TurnAbortReason::Interrupted {
            self.maybe_start_turn_for_pending_work().await;
        }
    }

    async fn take_active_turn(&self) -> Option<ActiveTurn> {
        let mut active = self.active_turn.lock().await;
        active.take()
    }
}

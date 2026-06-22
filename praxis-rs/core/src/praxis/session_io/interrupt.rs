use std::sync::Arc;

use praxis_protocol::protocol::TurnAbortReason;
use tracing::info;

use super::super::Session;

impl Session {
    pub async fn interrupt_task(self: &Arc<Self>) {
        info!("interrupt received: abort current task, if any");
        let has_active_turn = { self.active_turn.lock().await.is_some() };
        if has_active_turn {
            self.abort_all_tasks(TurnAbortReason::Interrupted).await;
        } else {
            self.cancel_mcp_startup().await;
        }
    }
}

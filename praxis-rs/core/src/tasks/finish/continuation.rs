use std::sync::Arc;

use tracing::warn;

use crate::goals::GoalRuntimeEvent;
use crate::praxis::Session;

impl Session {
    pub(super) fn schedule_pending_work_continuation(self: &Arc<Self>) {
        let session = Arc::clone(self);
        let _scheduler = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                session.maybe_start_turn_for_pending_work().await;
                if let Err(err) = session
                    .goal_runtime_apply(GoalRuntimeEvent::MaybeContinueIfIdle)
                    .await
                {
                    warn!("failed to apply goal idle-continuation runtime event: {err}");
                }
            });
        });
    }
}

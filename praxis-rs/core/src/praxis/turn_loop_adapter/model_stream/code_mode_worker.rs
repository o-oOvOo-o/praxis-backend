use std::sync::Arc;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::ToolRouter;
use crate::tools::context::SharedTurnDiffTracker;

pub(super) async fn start_turn_worker(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    router: Arc<ToolRouter>,
    turn_diff_tracker: SharedTurnDiffTracker,
) -> Option<praxis_code_mode::CodeModeTurnWorker> {
    session
        .services
        .code_mode_service
        .start_turn_worker(session, turn_context, router, turn_diff_tracker)
        .await
}

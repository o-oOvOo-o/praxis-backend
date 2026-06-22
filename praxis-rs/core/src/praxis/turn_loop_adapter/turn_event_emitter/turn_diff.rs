use std::sync::Arc;

use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::TurnDiffEvent;

use crate::tools::context::SharedTurnDiffTracker;

use super::super::super::Session;
use super::super::super::TurnContext;

pub(in crate::praxis::turn_loop_adapter) async fn emit_turn_diff_if_present(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    turn_diff_tracker: &SharedTurnDiffTracker,
) {
    let unified_diff = {
        let mut tracker = turn_diff_tracker.lock().await;
        tracker.get_unified_diff()
    };

    if let Ok(Some(unified_diff)) = unified_diff {
        session
            .send_event(
                turn_context,
                EventMsg::TurnDiff(TurnDiffEvent { unified_diff }),
            )
            .await;
    }
}

use std::collections::HashSet;
use std::sync::Arc;

use praxis_loop::outcome::LoopResult;
use praxis_protocol::models::ResponseItem;
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::context::SharedTurnDiffTracker;

use super::super::error_bridge::model_error;
use super::tool_runtime::ModelRoundTools;
use super::tool_runtime::build_tool_runtime;

pub(super) async fn build_tools(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    turn_diff_tracker: SharedTurnDiffTracker,
    items: &[ResponseItem],
    explicitly_enabled_connectors: &HashSet<String>,
    cancellation_token: &CancellationToken,
) -> LoopResult<ModelRoundTools> {
    build_tool_runtime(
        Arc::clone(session),
        Arc::clone(turn_context),
        turn_diff_tracker,
        items,
        explicitly_enabled_connectors,
        Some(turn_context.turn_skills.outcome.as_ref()),
        cancellation_token,
    )
    .await
    .map_err(model_error)
}

use std::sync::Arc;

use praxis_loop::outcome::LoopResult;
use praxis_loop::services::ModelRequest;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::request_context_update;
use super::request_settings;

pub(super) async fn resolve_request_turn_context(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    request: &ModelRequest,
) -> LoopResult<Arc<TurnContext>> {
    let settings = request_settings::parse_round_settings(&request.settings)?;
    if !request_context_update::round_settings_change_context(turn_context, &settings) {
        return Ok(Arc::clone(turn_context));
    }

    Ok(Arc::new(
        request_context_update::apply_round_settings(session, turn_context, settings).await,
    ))
}

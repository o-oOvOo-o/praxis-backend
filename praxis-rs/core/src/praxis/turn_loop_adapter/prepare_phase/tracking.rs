use std::sync::Arc;

use praxis_analytics::TrackEventsContext;
use praxis_analytics::build_track_events_context;

use super::super::super::Session;
use super::super::super::TurnContext;

pub(super) fn build_prepare_tracking(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> TrackEventsContext {
    build_track_events_context(
        turn_context.model_info.slug.clone(),
        sess.conversation_id.to_string(),
        turn_context.sub_id.clone(),
    )
}

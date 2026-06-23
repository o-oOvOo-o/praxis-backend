use praxis_protocol::protocol::RolloutItem;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::token_info::restore_last_token_info_from_rollout;

pub(super) async fn record(
    session: &Session,
    turn_context: &TurnContext,
    rollout_items: Vec<RolloutItem>,
    is_subagent: bool,
) {
    let previous_turn_settings = session
        .apply_rollout_reconstruction(turn_context, &rollout_items)
        .await;

    maybe_warn_on_model_change(session, turn_context, previous_turn_settings).await;
    restore_last_token_info_from_rollout(session, &rollout_items).await;

    if !is_subagent {
        session.flush_rollout().await;
    }
}

async fn maybe_warn_on_model_change(
    session: &Session,
    turn_context: &TurnContext,
    previous_turn_settings: Option<crate::praxis::PreviousTurnSettings>,
) {
    let curr: &str = turn_context.model_info.slug.as_str();
    if let Some(prev) = previous_turn_settings
        .as_ref()
        .map(|settings| settings.model.as_str())
        .filter(|model| *model != curr)
    {
        warn!("resuming session with different model: previous={prev}, current={curr}");
        session
            .turn_event_emitter(turn_context)
            .warning(format!(
                "This session was recorded with model `{prev}` but is resuming with `{curr}`. \
                         Consider switching back to `{prev}` as it may affect Praxis performance."
            ))
            .await;
    }
}

use praxis_protocol::protocol::RolloutItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::token_info::restore_last_token_info_from_rollout;

pub(super) async fn record(
    session: &Session,
    turn_context: &TurnContext,
    rollout_items: Vec<RolloutItem>,
    is_subagent: bool,
) {
    session
        .apply_rollout_reconstruction(turn_context, &rollout_items)
        .await;

    restore_last_token_info_from_rollout(session, &rollout_items).await;

    if !rollout_items.is_empty() {
        session.persist_rollout_items(&rollout_items).await;
    }

    session.ensure_rollout_materialized().await;

    if !is_subagent {
        session.flush_rollout().await;
    }
}

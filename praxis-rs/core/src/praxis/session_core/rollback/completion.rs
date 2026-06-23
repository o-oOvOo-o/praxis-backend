use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn commit(
    session: &Session,
    turn_context: &TurnContext,
    rollback_msg: EventMsg,
    replay_items: Vec<RolloutItem>,
) {
    session
        .persist_rollout_items(&[RolloutItem::EventMsg(rollback_msg.clone())])
        .await;
    session.flush_rollout().await;
    session
        .apply_rollout_reconstruction(turn_context, replay_items.as_slice())
        .await;
    session.recompute_token_usage(turn_context).await;

    session
        .deliver_event_raw(Event {
            id: turn_context.sub_id.clone(),
            msg: rollback_msg,
        })
        .await;
}

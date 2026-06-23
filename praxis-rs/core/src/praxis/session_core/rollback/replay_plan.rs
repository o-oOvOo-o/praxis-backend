use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::ThreadRolledBackEvent;

pub(super) fn rollback_message(event: ThreadRolledBackEvent) -> EventMsg {
    EventMsg::ThreadRolledBack(event)
}

pub(super) fn build_items(
    rollout_history: InitialHistory,
    rollback_msg: EventMsg,
) -> Vec<RolloutItem> {
    rollout_history
        .get_rollout_items()
        .into_iter()
        .chain(std::iter::once(RolloutItem::EventMsg(rollback_msg)))
        .collect::<Vec<_>>()
}

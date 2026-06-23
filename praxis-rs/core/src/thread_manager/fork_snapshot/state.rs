use crate::rollout::truncation;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::turn_lifecycle::TurnLifecycleStatus;
use praxis_protocol::turn_lifecycle::TurnLifecycleTracker;

#[derive(Debug, Eq, PartialEq)]
pub(in crate::thread_manager) struct SnapshotTurnState {
    pub(in crate::thread_manager) ends_mid_turn: bool,
    pub(in crate::thread_manager) active_turn_id: Option<String>,
    pub(in crate::thread_manager) active_turn_start_index: Option<usize>,
}

pub(in crate::thread_manager) fn snapshot_turn_state(
    history: &InitialHistory,
) -> SnapshotTurnState {
    let rollout_items = history.get_rollout_items();
    let mut builder = TurnLifecycleTracker::new();
    for item in &rollout_items {
        builder.handle_rollout_item(item);
    }
    let active_turn_id = builder.active_turn_id_if_explicit();
    if builder.has_active_turn() && active_turn_id.is_some() {
        let active_turn_snapshot = builder.active_turn_snapshot();
        if active_turn_snapshot
            .as_ref()
            .is_some_and(|turn| turn.status != TurnLifecycleStatus::InProgress)
        {
            return SnapshotTurnState {
                ends_mid_turn: false,
                active_turn_id: None,
                active_turn_start_index: None,
            };
        }

        return SnapshotTurnState {
            ends_mid_turn: true,
            active_turn_id,
            active_turn_start_index: builder.active_turn_start_index(),
        };
    }

    let Some(last_user_position) = truncation::user_message_positions_in_rollout(&rollout_items)
        .last()
        .copied()
    else {
        return SnapshotTurnState {
            ends_mid_turn: false,
            active_turn_id: None,
            active_turn_start_index: None,
        };
    };

    // Synthetic fork/resume histories can contain user/assistant response items
    // without explicit turn lifecycle events. If the persisted snapshot has no
    // terminating boundary after its last user message, treat it as mid-turn.
    SnapshotTurnState {
        ends_mid_turn: !rollout_items[last_user_position + 1..].iter().any(|item| {
            matches!(
                item,
                RolloutItem::EventMsg(EventMsg::TurnComplete(_) | EventMsg::TurnAborted(_))
            )
        }),
        active_turn_id: None,
        active_turn_start_index: None,
    }
}

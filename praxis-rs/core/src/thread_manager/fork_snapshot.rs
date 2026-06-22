use crate::rollout::truncation;
use crate::tasks::interrupted_turn_history_marker;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::protocol::TurnAbortedEvent;
use praxis_protocol::turn_lifecycle::TurnLifecycleStatus;
use praxis_protocol::turn_lifecycle::TurnLifecycleTracker;

/// Return a fork snapshot cut strictly before the nth user message (0-based).
///
/// Out-of-range values keep the full committed history at a turn boundary, but
/// when the source thread is currently mid-turn they fall back to cutting
/// before the active turn's opening boundary so the fork omits the unfinished
/// suffix entirely.
pub(super) fn truncate_before_nth_user_message(
    history: InitialHistory,
    n: usize,
    snapshot_state: &SnapshotTurnState,
) -> InitialHistory {
    let items: Vec<RolloutItem> = history.get_rollout_items();
    let user_positions = truncation::user_message_positions_in_rollout(&items);
    let rolled = if snapshot_state.ends_mid_turn && n >= user_positions.len() {
        if let Some(cut_idx) = snapshot_state
            .active_turn_start_index
            .or_else(|| user_positions.last().copied())
        {
            items[..cut_idx].to_vec()
        } else {
            items
        }
    } else {
        truncation::truncate_rollout_before_nth_user_message_from_start(&items, n)
    };

    if rolled.is_empty() {
        InitialHistory::New
    } else {
        InitialHistory::Forked(rolled)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct SnapshotTurnState {
    pub(super) ends_mid_turn: bool,
    pub(super) active_turn_id: Option<String>,
    pub(super) active_turn_start_index: Option<usize>,
}

pub(super) fn snapshot_turn_state(history: &InitialHistory) -> SnapshotTurnState {
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

/// Append the same persisted interrupt boundary used by the live interrupt path
/// to an existing fork snapshot after the source thread has been confirmed to
/// be mid-turn.
pub(super) fn append_interrupted_boundary(
    history: InitialHistory,
    turn_id: Option<String>,
) -> InitialHistory {
    let aborted_event = RolloutItem::EventMsg(EventMsg::TurnAborted(TurnAbortedEvent {
        turn_id,
        reason: TurnAbortReason::Interrupted,
    }));

    match history {
        InitialHistory::New => InitialHistory::Forked(vec![
            RolloutItem::ResponseItem(interrupted_turn_history_marker()),
            aborted_event,
        ]),
        InitialHistory::Forked(mut history) => {
            history.push(RolloutItem::ResponseItem(interrupted_turn_history_marker()));
            history.push(aborted_event);
            InitialHistory::Forked(history)
        }
        InitialHistory::Resumed(mut resumed) => {
            resumed
                .history
                .push(RolloutItem::ResponseItem(interrupted_turn_history_marker()));
            resumed.history.push(aborted_event);
            InitialHistory::Forked(resumed.history)
        }
    }
}

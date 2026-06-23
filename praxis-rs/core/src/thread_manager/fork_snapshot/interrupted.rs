use crate::tasks::interrupted_turn_history_marker;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::protocol::TurnAbortedEvent;

/// Append the same persisted interrupt boundary used by the live interrupt path
/// to an existing fork snapshot after the source thread has been confirmed to
/// be mid-turn.
pub(in crate::thread_manager) fn append_interrupted_boundary(
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

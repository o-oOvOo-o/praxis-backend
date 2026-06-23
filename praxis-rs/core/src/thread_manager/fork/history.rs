use praxis_protocol::protocol::InitialHistory;

use super::snapshot_mode::ThreadForkSnapshot;
use crate::thread_manager::fork_snapshot::append_interrupted_boundary;
use crate::thread_manager::fork_snapshot::snapshot_turn_state;
use crate::thread_manager::fork_snapshot::truncate_before_nth_user_message;

pub(super) fn fork_initial_history(
    snapshot: ThreadForkSnapshot,
    history: InitialHistory,
) -> InitialHistory {
    let snapshot_state = snapshot_turn_state(&history);
    match snapshot {
        ThreadForkSnapshot::TruncateBeforeNthUserMessage(nth_user_message) => {
            truncate_before_nth_user_message(history, nth_user_message, &snapshot_state)
        }
        ThreadForkSnapshot::Interrupted => {
            let history = committed_fork_history(history);
            if snapshot_state.ends_mid_turn {
                append_interrupted_boundary(history, snapshot_state.active_turn_id)
            } else {
                history
            }
        }
    }
}

fn committed_fork_history(history: InitialHistory) -> InitialHistory {
    match history {
        InitialHistory::New => InitialHistory::New,
        InitialHistory::Forked(history) => InitialHistory::Forked(history),
        InitialHistory::Resumed(resumed) => InitialHistory::Forked(resumed.history),
    }
}

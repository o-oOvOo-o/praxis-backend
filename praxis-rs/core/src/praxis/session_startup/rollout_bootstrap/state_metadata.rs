use praxis_protocol::protocol::InitialHistory;
use praxis_state::ThreadMetadataBuilder;

use crate::rollout::metadata;

pub(super) fn builder_from_initial_history(
    initial_history: &InitialHistory,
) -> Option<ThreadMetadataBuilder> {
    match initial_history {
        InitialHistory::Resumed(resumed) => {
            metadata::builder_from_items(resumed.history.as_slice(), resumed.rollout_path.as_path())
        }
        InitialHistory::New | InitialHistory::Forked(_) => None,
    }
}

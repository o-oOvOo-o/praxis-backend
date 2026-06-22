mod materialize;
mod scan;
mod segment;
mod types;

use praxis_protocol::protocol::RolloutItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use self::materialize::materialize_history_from_replay;
use self::scan::scan_rollout_replay;
use self::types::RolloutReconstruction;

impl Session {
    pub(super) async fn reconstruct_history_from_rollout(
        &self,
        turn_context: &TurnContext,
        rollout_items: &[RolloutItem],
    ) -> RolloutReconstruction {
        let replay = scan_rollout_replay(rollout_items);
        let materialized = materialize_history_from_replay(
            turn_context,
            replay.base_replacement_history,
            replay.rollout_suffix,
        );
        let reference_context_item = if materialized.saw_legacy_compaction_without_replacement {
            None
        } else {
            replay.reference_context_item.into_resolved()
        };

        RolloutReconstruction {
            history: materialized.history,
            previous_turn_settings: replay.previous_turn_settings,
            reference_context_item,
        }
    }
}

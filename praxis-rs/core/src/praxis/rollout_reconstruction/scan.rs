mod item;
mod scanner;

use praxis_protocol::protocol::RolloutItem;

use super::types::RolloutReplayPlan;
use scanner::RolloutReplayScanner;

pub(super) fn scan_rollout_replay(rollout_items: &[RolloutItem]) -> RolloutReplayPlan<'_> {
    RolloutReplayScanner::new(rollout_items).scan()
}

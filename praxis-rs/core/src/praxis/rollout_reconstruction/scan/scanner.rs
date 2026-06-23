use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::RolloutItem;

use crate::praxis::PreviousTurnSettings;

use super::super::segment::finalize_active_segment;
use super::super::types::ActiveReplaySegment;
use super::super::types::RolloutReplayPlan;
use super::super::types::TurnReferenceContextItem;

pub(super) struct RolloutReplayScanner<'a> {
    pub(super) rollout_items: &'a [RolloutItem],
    pub(super) base_replacement_history: Option<&'a [ResponseItem]>,
    pub(super) previous_turn_settings: Option<PreviousTurnSettings>,
    pub(super) reference_context_item: TurnReferenceContextItem,
    pub(super) pending_rollback_turns: usize,
    pub(super) rollout_suffix: &'a [RolloutItem],
    pub(super) active_segment: Option<ActiveReplaySegment<'a>>,
}

impl<'a> RolloutReplayScanner<'a> {
    pub(super) fn new(rollout_items: &'a [RolloutItem]) -> Self {
        Self {
            rollout_items,
            base_replacement_history: None,
            previous_turn_settings: None,
            reference_context_item: TurnReferenceContextItem::NeverSet,
            pending_rollback_turns: 0,
            rollout_suffix: rollout_items,
            active_segment: None,
        }
    }

    pub(super) fn scan(mut self) -> RolloutReplayPlan<'a> {
        for (index, item) in self.rollout_items.iter().enumerate().rev() {
            self.scan_item(item, index);
            if self.has_enough_metadata() {
                break;
            }
        }
        self.finalize_active_segment();
        self.into_plan()
    }

    pub(super) fn finalize_active_segment(&mut self) {
        if let Some(active_segment) = self.active_segment.take() {
            finalize_active_segment(
                active_segment,
                &mut self.base_replacement_history,
                &mut self.previous_turn_settings,
                &mut self.reference_context_item,
                &mut self.pending_rollback_turns,
            );
        }
    }

    fn has_enough_metadata(&self) -> bool {
        self.base_replacement_history.is_some()
            && self.previous_turn_settings.is_some()
            && !self.reference_context_item.is_never_set()
    }

    fn into_plan(self) -> RolloutReplayPlan<'a> {
        RolloutReplayPlan {
            base_replacement_history: self.base_replacement_history,
            previous_turn_settings: self.previous_turn_settings,
            reference_context_item: self.reference_context_item,
            rollout_suffix: self.rollout_suffix,
        }
    }
}

use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;

use crate::context_manager::is_user_turn_boundary;
use crate::praxis::PreviousTurnSettings;

use super::scanner::RolloutReplayScanner;
use crate::praxis::rollout_reconstruction::segment::turn_ids_are_compatible;
use crate::praxis::rollout_reconstruction::types::ActiveReplaySegment;
use crate::praxis::rollout_reconstruction::types::TurnReferenceContextItem;

impl<'a> RolloutReplayScanner<'a> {
    pub(super) fn scan_item(&mut self, item: &'a RolloutItem, index: usize) {
        match item {
            RolloutItem::Compacted(compacted) => {
                let active_segment = self.active_segment();
                if active_segment.reference_context_item.is_never_set() {
                    active_segment.reference_context_item = TurnReferenceContextItem::Cleared;
                }
                if active_segment.base_replacement_history.is_none()
                    && let Some(replacement_history) = &compacted.replacement_history
                {
                    active_segment.base_replacement_history = Some(replacement_history);
                    self.rollout_suffix = &self.rollout_items[index + 1..];
                }
            }
            RolloutItem::EventMsg(EventMsg::ThreadRolledBack(rollback)) => {
                self.pending_rollback_turns = self
                    .pending_rollback_turns
                    .saturating_add(usize::try_from(rollback.num_turns).unwrap_or(usize::MAX));
            }
            RolloutItem::EventMsg(EventMsg::TurnComplete(event)) => {
                let active_segment = self.active_segment();
                if active_segment.turn_id.is_none() {
                    active_segment.turn_id = Some(event.turn_id.clone());
                }
            }
            RolloutItem::EventMsg(EventMsg::TurnAborted(event)) => {
                self.scan_turn_aborted(event.turn_id.as_deref());
            }
            RolloutItem::EventMsg(EventMsg::UserMessage(_)) => {
                self.active_segment().counts_as_user_turn = true;
            }
            RolloutItem::TurnContext(ctx) => {
                let active_segment = self.active_segment();
                if active_segment.turn_id.is_none() {
                    active_segment.turn_id = ctx.turn_id.clone();
                }
                if turn_ids_are_compatible(
                    active_segment.turn_id.as_deref(),
                    ctx.turn_id.as_deref(),
                ) {
                    active_segment.previous_turn_settings = Some(PreviousTurnSettings {
                        model: ctx.model.clone(),
                        realtime_active: ctx.realtime_active,
                    });
                    if active_segment.reference_context_item.is_never_set() {
                        active_segment.reference_context_item =
                            TurnReferenceContextItem::Latest(Box::new(ctx.clone()));
                    }
                }
            }
            RolloutItem::EventMsg(EventMsg::TurnStarted(event)) => {
                let matches_active_segment = self.active_segment.as_ref().is_some_and(|segment| {
                    turn_ids_are_compatible(
                        segment.turn_id.as_deref(),
                        Some(event.turn_id.as_str()),
                    )
                });
                if matches_active_segment {
                    self.finalize_active_segment();
                }
            }
            RolloutItem::ResponseItem(response_item) => {
                let active_segment = self.active_segment();
                active_segment.counts_as_user_turn |= is_user_turn_boundary(response_item);
            }
            RolloutItem::EventMsg(_) | RolloutItem::SessionMeta(_) => {}
        }
    }

    fn active_segment(&mut self) -> &mut ActiveReplaySegment<'a> {
        self.active_segment
            .get_or_insert_with(ActiveReplaySegment::default)
    }

    fn scan_turn_aborted(&mut self, turn_id: Option<&str>) {
        if let Some(active_segment) = self.active_segment.as_mut() {
            if active_segment.turn_id.is_none()
                && let Some(turn_id) = turn_id
            {
                active_segment.turn_id = Some(turn_id.to_string());
            }
        } else if let Some(turn_id) = turn_id {
            self.active_segment = Some(ActiveReplaySegment {
                turn_id: Some(turn_id.to_string()),
                ..Default::default()
            });
        }
    }
}

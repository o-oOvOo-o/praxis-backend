use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;

use crate::context_manager::is_user_turn_boundary;
use crate::praxis::PreviousTurnSettings;

use super::segment::finalize_active_segment;
use super::segment::turn_ids_are_compatible;
use super::types::ActiveReplaySegment;
use super::types::RolloutReplayPlan;
use super::types::TurnReferenceContextItem;

pub(super) fn scan_rollout_replay(rollout_items: &[RolloutItem]) -> RolloutReplayPlan<'_> {
    let mut base_replacement_history: Option<&[ResponseItem]> = None;
    let mut previous_turn_settings = None;
    let mut reference_context_item = TurnReferenceContextItem::NeverSet;
    let mut pending_rollback_turns = 0usize;
    let mut rollout_suffix = rollout_items;
    let mut active_segment: Option<ActiveReplaySegment<'_>> = None;

    for (index, item) in rollout_items.iter().enumerate().rev() {
        scan_item(
            item,
            index,
            rollout_items,
            &mut active_segment,
            &mut base_replacement_history,
            &mut previous_turn_settings,
            &mut reference_context_item,
            &mut pending_rollback_turns,
            &mut rollout_suffix,
        );

        if scan_has_enough_metadata(
            base_replacement_history,
            previous_turn_settings.as_ref(),
            &reference_context_item,
        ) {
            break;
        }
    }

    if let Some(active_segment) = active_segment.take() {
        finalize_active_segment(
            active_segment,
            &mut base_replacement_history,
            &mut previous_turn_settings,
            &mut reference_context_item,
            &mut pending_rollback_turns,
        );
    }

    RolloutReplayPlan {
        base_replacement_history,
        previous_turn_settings,
        reference_context_item,
        rollout_suffix,
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_item<'a>(
    item: &'a RolloutItem,
    index: usize,
    rollout_items: &'a [RolloutItem],
    active_segment: &mut Option<ActiveReplaySegment<'a>>,
    base_replacement_history: &mut Option<&'a [ResponseItem]>,
    previous_turn_settings: &mut Option<PreviousTurnSettings>,
    reference_context_item: &mut TurnReferenceContextItem,
    pending_rollback_turns: &mut usize,
    rollout_suffix: &mut &'a [RolloutItem],
) {
    match item {
        RolloutItem::Compacted(compacted) => {
            let active_segment = active_segment.get_or_insert_with(ActiveReplaySegment::default);
            if active_segment.reference_context_item.is_never_set() {
                active_segment.reference_context_item = TurnReferenceContextItem::Cleared;
            }
            if active_segment.base_replacement_history.is_none()
                && let Some(replacement_history) = &compacted.replacement_history
            {
                active_segment.base_replacement_history = Some(replacement_history);
                *rollout_suffix = &rollout_items[index + 1..];
            }
        }
        RolloutItem::EventMsg(EventMsg::ThreadRolledBack(rollback)) => {
            *pending_rollback_turns = pending_rollback_turns
                .saturating_add(usize::try_from(rollback.num_turns).unwrap_or(usize::MAX));
        }
        RolloutItem::EventMsg(EventMsg::TurnComplete(event)) => {
            let active_segment = active_segment.get_or_insert_with(ActiveReplaySegment::default);
            if active_segment.turn_id.is_none() {
                active_segment.turn_id = Some(event.turn_id.clone());
            }
        }
        RolloutItem::EventMsg(EventMsg::TurnAborted(event)) => {
            scan_turn_aborted(event.turn_id.as_deref(), active_segment);
        }
        RolloutItem::EventMsg(EventMsg::UserMessage(_)) => {
            let active_segment = active_segment.get_or_insert_with(ActiveReplaySegment::default);
            active_segment.counts_as_user_turn = true;
        }
        RolloutItem::TurnContext(ctx) => {
            let active_segment = active_segment.get_or_insert_with(ActiveReplaySegment::default);
            if active_segment.turn_id.is_none() {
                active_segment.turn_id = ctx.turn_id.clone();
            }
            if turn_ids_are_compatible(active_segment.turn_id.as_deref(), ctx.turn_id.as_deref()) {
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
            let matches_active_segment = active_segment.as_ref().is_some_and(|active_segment| {
                turn_ids_are_compatible(
                    active_segment.turn_id.as_deref(),
                    Some(event.turn_id.as_str()),
                )
            });
            if matches_active_segment && let Some(active_segment) = active_segment.take() {
                finalize_active_segment(
                    active_segment,
                    base_replacement_history,
                    previous_turn_settings,
                    reference_context_item,
                    pending_rollback_turns,
                );
            }
        }
        RolloutItem::ResponseItem(response_item) => {
            let active_segment = active_segment.get_or_insert_with(ActiveReplaySegment::default);
            active_segment.counts_as_user_turn |= is_user_turn_boundary(response_item);
        }
        RolloutItem::EventMsg(_) | RolloutItem::SessionMeta(_) => {}
    }
}

fn scan_turn_aborted<'a>(
    turn_id: Option<&str>,
    active_segment: &mut Option<ActiveReplaySegment<'a>>,
) {
    if let Some(active_segment) = active_segment.as_mut() {
        if active_segment.turn_id.is_none()
            && let Some(turn_id) = turn_id
        {
            active_segment.turn_id = Some(turn_id.to_string());
        }
    } else if let Some(turn_id) = turn_id {
        *active_segment = Some(ActiveReplaySegment {
            turn_id: Some(turn_id.to_string()),
            ..Default::default()
        });
    }
}

fn scan_has_enough_metadata(
    base_replacement_history: Option<&[ResponseItem]>,
    previous_turn_settings: Option<&PreviousTurnSettings>,
    reference_context_item: &TurnReferenceContextItem,
) -> bool {
    base_replacement_history.is_some()
        && previous_turn_settings.is_some()
        && !reference_context_item.is_never_set()
}

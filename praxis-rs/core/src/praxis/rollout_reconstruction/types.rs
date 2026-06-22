use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::TurnContextItem;

use crate::praxis::PreviousTurnSettings;

// Bundles rebuilt history with resume/fork hydration metadata derived from the same replay.
#[derive(Debug)]
pub(in crate::praxis) struct RolloutReconstruction {
    pub(in crate::praxis) history: Vec<ResponseItem>,
    pub(in crate::praxis) previous_turn_settings: Option<PreviousTurnSettings>,
    pub(in crate::praxis) reference_context_item: Option<TurnContextItem>,
}

#[derive(Debug, Default)]
pub(super) enum TurnReferenceContextItem {
    /// No `TurnContextItem` has been seen for this replay span yet.
    #[default]
    NeverSet,
    /// A previously established baseline was invalidated by later compaction.
    Cleared,
    /// The latest baseline established by this replay span.
    Latest(Box<TurnContextItem>),
}

impl TurnReferenceContextItem {
    pub(super) fn is_never_set(&self) -> bool {
        matches!(self, Self::NeverSet)
    }

    pub(super) fn into_resolved(self) -> Option<TurnContextItem> {
        match self {
            Self::NeverSet | Self::Cleared => None,
            Self::Latest(turn_reference_context_item) => Some(*turn_reference_context_item),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ActiveReplaySegment<'a> {
    pub(super) turn_id: Option<String>,
    pub(super) counts_as_user_turn: bool,
    pub(super) previous_turn_settings: Option<PreviousTurnSettings>,
    pub(super) reference_context_item: TurnReferenceContextItem,
    pub(super) base_replacement_history: Option<&'a [ResponseItem]>,
}

#[derive(Debug)]
pub(super) struct RolloutReplayPlan<'a> {
    pub(super) base_replacement_history: Option<&'a [ResponseItem]>,
    pub(super) previous_turn_settings: Option<PreviousTurnSettings>,
    pub(super) reference_context_item: TurnReferenceContextItem,
    pub(super) rollout_suffix: &'a [RolloutItem],
}

#[derive(Debug)]
pub(super) struct MaterializedHistory {
    pub(super) history: Vec<ResponseItem>,
    pub(super) saw_legacy_compaction_without_replacement: bool,
}

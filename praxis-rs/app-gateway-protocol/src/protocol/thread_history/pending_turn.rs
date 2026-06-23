use super::*;

pub(super) fn upsert_turn_item(items: &mut Vec<ThreadItem>, item: ThreadItem) {
    if let Some(existing_item) = items
        .iter_mut()
        .find(|existing_item| existing_item.id() == item.id())
    {
        *existing_item = item;
        return;
    }
    items.push(item);
}

pub(super) struct PendingTurn {
    pub(super) id: String,
    pub(super) items: Vec<ThreadItem>,
    pub(super) error: Option<TurnError>,
    pub(super) status: TurnStatus,
    /// True when this turn originated from an explicit `turn_started`/`turn_complete`
    /// boundary, so we preserve it even if it has no renderable items.
    pub(super) opened_explicitly: bool,
    /// True when this turn includes a persisted `RolloutItem::Compacted`, which
    /// should keep the turn from being dropped even without normal items.
    pub(super) saw_compaction: bool,
    /// Index of the rollout item that opened this turn during replay.
    pub(super) rollout_start_index: usize,
}

impl PendingTurn {
    pub(super) fn opened_explicitly(mut self) -> Self {
        self.opened_explicitly = true;
        self
    }

    pub(super) fn with_status(mut self, status: TurnStatus) -> Self {
        self.status = status;
        self
    }
}

impl From<PendingTurn> for Turn {
    fn from(value: PendingTurn) -> Self {
        Self {
            id: value.id,
            items: value.items,
            error: value.error,
            status: value.status,
        }
    }
}

impl From<&PendingTurn> for Turn {
    fn from(value: &PendingTurn) -> Self {
        Self {
            id: value.id.clone(),
            items: value.items.clone(),
            error: value.error.clone(),
            status: value.status.clone(),
        }
    }
}

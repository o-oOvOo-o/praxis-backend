use super::*;

impl ThreadHistoryBuilder {
    /// Marks the current turn as containing a persisted compaction marker.
    ///
    /// This keeps compaction-only legacy turns from being dropped by
    /// `finish_current_turn` when they have no renderable items and were not
    /// explicitly opened.
    pub(super) fn handle_compacted(&mut self, _payload: &CompactedItem) {
        self.ensure_turn().saw_compaction = true;
    }

    pub(super) fn handle_thread_rollback(&mut self, payload: &ThreadRolledBackEvent) {
        self.finish_current_turn();

        let mut remaining = usize::try_from(payload.num_turns).unwrap_or(usize::MAX);
        while remaining > 0 {
            if self.turns.pop().is_some() {
                remaining = remaining.saturating_sub(1);
                continue;
            }
            if self.dropped_turns.pop_back().is_some() {
                remaining = remaining.saturating_sub(1);
                continue;
            }
            break;
        }
        self.refill_finished_turn_window();

        let item_count = self.effective_finished_item_count();
        self.next_item_index = i64::try_from(item_count.saturating_add(1)).unwrap_or(i64::MAX);
    }

    pub(super) fn finish_current_turn(&mut self) {
        if let Some(turn) = self.current_turn.take() {
            if turn.items.is_empty() && !turn.opened_explicitly && !turn.saw_compaction {
                return;
            }
            self.turns.push(turn.into());
        }
    }

    pub(super) fn trim_finished_turns(&mut self) {
        let Some(max_finished_turns) = self.max_finished_turns else {
            return;
        };
        while self.turns.len() > max_finished_turns {
            let turn = self.turns.remove(0);
            self.dropped_turns.push_back(turn);
        }
    }

    pub(super) fn refill_finished_turn_window(&mut self) {
        let Some(max_finished_turns) = self.max_finished_turns else {
            return;
        };
        while self.turns.len() < max_finished_turns {
            let Some(turn) = self.dropped_turns.pop_back() else {
                break;
            };
            self.turns.insert(0, turn);
        }
    }

    pub(super) fn effective_finished_item_count(&self) -> usize {
        let dropped_item_count: usize =
            self.dropped_turns.iter().map(|turn| turn.items.len()).sum();
        let retained_item_count: usize = self.turns.iter().map(|turn| turn.items.len()).sum();
        dropped_item_count.saturating_add(retained_item_count)
    }

    pub(super) fn new_turn(&mut self, id: Option<String>) -> PendingTurn {
        PendingTurn {
            id: id.unwrap_or_else(|| Uuid::now_v7().to_string()),
            collaboration_mode_kind: Default::default(),
            items: Vec::new(),
            error: None,
            status: TurnStatus::Completed,
            opened_explicitly: false,
            saw_compaction: false,
            rollout_start_index: self.current_rollout_index,
        }
    }

    pub(super) fn ensure_turn(&mut self) -> &mut PendingTurn {
        if self.current_turn.is_none() {
            let turn = self.new_turn(/*id*/ None);
            return self.current_turn.insert(turn);
        }

        if let Some(turn) = self.current_turn.as_mut() {
            return turn;
        }

        unreachable!("current turn must exist after initialization");
    }

    pub(super) fn upsert_item_in_turn_id(&mut self, turn_id: &str, item: ThreadItem) {
        if let Some(turn) = self.current_turn.as_mut()
            && turn.id == turn_id
        {
            upsert_turn_item(&mut turn.items, item);
            return;
        }

        if let Some(turn) = self.turns.iter_mut().find(|turn| turn.id == turn_id) {
            upsert_turn_item(&mut turn.items, item);
            return;
        }

        warn!(
            item_id = item.id(),
            "dropping turn-scoped item for unknown turn id `{turn_id}`"
        );
    }

    pub(super) fn record_canonical_user_message(&mut self, turn_id: &str, item: ThreadItem) {
        let replace_in = |items: &mut Vec<ThreadItem>| {
            if let Some(last) = items.last_mut()
                && matches!(
                    (&*last, &item),
                    (
                        ThreadItem::UserMessage { content: existing, .. },
                        ThreadItem::UserMessage { content: canonical, .. }
                    ) if existing == canonical
                )
            {
                *last = item;
            } else {
                upsert_turn_item(items, item);
            }
        };

        if let Some(turn) = self.current_turn.as_mut()
            && turn.id == turn_id
        {
            replace_in(&mut turn.items);
            return;
        }
        if let Some(turn) = self.turns.iter_mut().find(|turn| turn.id == turn_id) {
            replace_in(&mut turn.items);
            return;
        }
        warn!("dropping canonical user message for unknown turn id `{turn_id}`");
    }

    pub(super) fn upsert_item_in_current_turn(&mut self, item: ThreadItem) {
        let turn = self.ensure_turn();
        upsert_turn_item(&mut turn.items, item);
    }

    pub(super) fn next_item_id(&mut self) -> String {
        let id = format!("item-{}", self.next_item_index);
        self.next_item_index += 1;
        id
    }

    pub(super) fn build_user_inputs(&self, payload: &UserMessageEvent) -> Vec<UserInput> {
        let mut content = Vec::new();
        if !payload.message.trim().is_empty() {
            content.push(UserInput::Text {
                text: payload.message.clone(),
                text_elements: payload
                    .text_elements
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect(),
            });
        }
        if let Some(images) = &payload.images {
            for image in images {
                content.push(UserInput::Image { url: image.clone() });
            }
        }
        for path in &payload.local_images {
            content.push(UserInput::LocalImage { path: path.clone() });
        }
        content
    }
}

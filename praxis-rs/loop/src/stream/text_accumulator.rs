use crate::model::TurnItem;
use crate::state::TurnState;

#[derive(Debug, Default)]
pub(super) struct AssistantTextAccumulator {
    item_id: Option<String>,
    text: String,
    record_state: AssistantTextRecordState,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AssistantTextRecordState {
    #[default]
    NeedsRecord,
    AlreadyRecorded,
}

impl AssistantTextAccumulator {
    pub(super) fn push_delta(&mut self, item_id: &Option<String>, text: &str) {
        self.capture_item_id(item_id.clone());
        self.text.push_str(text);
    }

    pub(super) fn push_final(&mut self, item_id: Option<String>, text: String) {
        self.capture_item_id(item_id);
        self.text.push_str(&text);
    }

    pub(super) fn push_recorded_final(&mut self, item_id: Option<String>, text: String) {
        self.push_final(item_id, text);
        self.record_state = AssistantTextRecordState::AlreadyRecorded;
    }

    pub(super) fn commit(
        self,
        state: &mut TurnState,
        new_items: &mut Vec<TurnItem>,
    ) -> Option<String> {
        if self.text.is_empty() {
            return None;
        }

        state.record_last_agent_message(self.text.clone());
        if self.record_state == AssistantTextRecordState::NeedsRecord {
            new_items.push(TurnItem::AssistantText {
                item_id: self.item_id,
                text: self.text.clone(),
            });
        }
        Some(self.text)
    }

    fn capture_item_id(&mut self, item_id: Option<String>) {
        if self.item_id.is_none() {
            self.item_id = item_id;
        }
    }
}

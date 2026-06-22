use crate::util::error_or_panic;

use super::super::Session;
use super::super::TurnContext;

pub(super) struct TurnEventScope {
    thread_id: String,
    turn_id: String,
}

impl TurnEventScope {
    pub(super) fn new(session: &Session, turn_context: &TurnContext) -> Self {
        Self {
            thread_id: session.conversation_id.to_string(),
            turn_id: turn_context.sub_id.clone(),
        }
    }

    pub(super) fn thread_id(&self) -> String {
        self.thread_id.clone()
    }

    pub(super) fn turn_id(&self) -> String {
        self.turn_id.clone()
    }

    pub(super) fn active_item_id(
        &self,
        item_id: Option<String>,
        event_name: &'static str,
    ) -> Option<String> {
        if item_id.is_some() {
            return item_id;
        }
        error_or_panic(format!("{event_name} without active item"));
        None
    }
}

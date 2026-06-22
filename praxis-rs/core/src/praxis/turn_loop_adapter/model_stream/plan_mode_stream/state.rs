use std::collections::HashMap;
use std::collections::HashSet;

use praxis_protocol::items::TurnItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::plan_item::ProposedPlanItemState;

pub(in crate::praxis::turn_loop_adapter) struct PlanModeStreamState {
    pending_agent_message_items: HashMap<String, TurnItem>,
    started_agent_message_items: HashSet<String>,
    leading_whitespace_by_item: HashMap<String, String>,
    plan_item_state: ProposedPlanItemState,
}

impl PlanModeStreamState {
    pub(in crate::praxis::turn_loop_adapter) fn new(turn_id: &str) -> Self {
        Self {
            pending_agent_message_items: HashMap::new(),
            started_agent_message_items: HashSet::new(),
            leading_whitespace_by_item: HashMap::new(),
            plan_item_state: ProposedPlanItemState::new(turn_id),
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn insert_pending_agent_message(
        &mut self,
        item_id: String,
        item: TurnItem,
    ) {
        self.pending_agent_message_items.insert(item_id, item);
    }

    pub(super) fn forget_agent_message(&mut self, item_id: &str) {
        self.pending_agent_message_items.remove(item_id);
        self.started_agent_message_items.remove(item_id);
        self.leading_whitespace_by_item.remove(item_id);
    }

    pub(super) fn agent_message_started(&self, item_id: &str) -> bool {
        self.started_agent_message_items.contains(item_id)
    }

    pub(super) fn mark_agent_message_started(&mut self, item_id: impl Into<String>) {
        self.started_agent_message_items.insert(item_id.into());
    }

    pub(super) fn clear_agent_message_started(&mut self, item_id: &str) {
        self.started_agent_message_items.remove(item_id);
    }

    pub(super) fn take_pending_agent_message(&mut self, item_id: &str) -> Option<TurnItem> {
        self.pending_agent_message_items.remove(item_id)
    }

    pub(super) fn push_leading_whitespace(&mut self, item_id: &str, delta: &str) {
        self.leading_whitespace_by_item
            .entry(item_id.to_string())
            .or_default()
            .push_str(delta);
    }

    pub(super) fn take_leading_whitespace(&mut self, item_id: &str) -> Option<String> {
        self.leading_whitespace_by_item.remove(item_id)
    }

    pub(super) async fn emit_pending_agent_message_start(
        &mut self,
        sess: &Session,
        turn_context: &TurnContext,
        item_id: &str,
    ) {
        if self.agent_message_started(item_id) {
            return;
        }
        if let Some(item) = self.take_pending_agent_message(item_id) {
            sess.emit_turn_item_started(turn_context, &item).await;
            self.mark_agent_message_started(item_id.to_string());
        }
    }

    pub(super) fn plan_item_started(&self) -> bool {
        self.plan_item_state.started
    }

    pub(super) fn plan_item_completed(&self) -> bool {
        self.plan_item_state.completed
    }

    pub(super) async fn start_plan_item(&mut self, sess: &Session, turn_context: &TurnContext) {
        self.plan_item_state.start(sess, turn_context).await;
    }

    pub(super) async fn push_plan_delta(
        &mut self,
        sess: &Session,
        turn_context: &TurnContext,
        delta: &str,
    ) {
        self.plan_item_state
            .push_delta(sess, turn_context, delta)
            .await;
    }

    pub(super) async fn complete_plan_item_with_text(
        &mut self,
        sess: &Session,
        turn_context: &TurnContext,
        text: String,
    ) {
        self.plan_item_state
            .complete_with_text(sess, turn_context, text)
            .await;
    }
}

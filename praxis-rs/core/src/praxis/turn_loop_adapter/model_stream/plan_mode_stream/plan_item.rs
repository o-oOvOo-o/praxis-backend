use praxis_protocol::items::PlanItem;
use praxis_protocol::items::TurnItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PlanDeltaEvent;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) struct ProposedPlanItemState {
    item_id: String,
    pub(super) started: bool,
    pub(super) completed: bool,
}

impl ProposedPlanItemState {
    pub(super) fn new(turn_id: &str) -> Self {
        Self {
            item_id: format!("{turn_id}-plan"),
            started: false,
            completed: false,
        }
    }

    pub(super) async fn start(&mut self, sess: &Session, turn_context: &TurnContext) {
        if self.started || self.completed {
            return;
        }
        self.started = true;
        let item = TurnItem::Plan(PlanItem {
            id: self.item_id.clone(),
            text: String::new(),
        });
        sess.emit_turn_item_started(turn_context, &item).await;
    }

    pub(super) async fn push_delta(
        &mut self,
        sess: &Session,
        turn_context: &TurnContext,
        delta: &str,
    ) {
        if self.completed || delta.is_empty() {
            return;
        }
        let event = PlanDeltaEvent {
            thread_id: sess.conversation_id.to_string(),
            turn_id: turn_context.sub_id.clone(),
            item_id: self.item_id.clone(),
            delta: delta.to_string(),
        };
        sess.send_event(turn_context, EventMsg::PlanDelta(event))
            .await;
    }

    pub(super) async fn complete_with_text(
        &mut self,
        sess: &Session,
        turn_context: &TurnContext,
        text: String,
    ) {
        if self.completed || !self.started {
            return;
        }
        self.completed = true;
        let item = TurnItem::Plan(PlanItem {
            id: self.item_id.clone(),
            text,
        });
        sess.emit_turn_item_completed(turn_context, item).await;
    }
}

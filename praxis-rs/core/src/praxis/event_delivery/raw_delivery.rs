use tracing::debug;

use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;

use crate::agent::agent_status_from_event;
use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    /// Persist the event to rollout and send it to clients.
    pub(crate) async fn send_event(&self, turn_context: &TurnContext, msg: EventMsg) {
        let event_msg = msg.clone();
        let event = Event {
            id: turn_context.sub_id.clone(),
            msg,
        };
        self.send_event_raw(event).await;
        self.maybe_notify_parent_of_terminal_turn(turn_context, &event_msg)
            .await;
        self.maybe_mirror_event_text_to_realtime(&event_msg)
            .await;
        self.maybe_clear_realtime_handoff_for_event(&event_msg)
            .await;
    }

    pub(crate) async fn send_event_raw(&self, event: Event) {
        let rollout_items = vec![RolloutItem::EventMsg(event.msg.clone())];
        self.persist_rollout_items(&rollout_items).await;
        self.deliver_event_raw(event).await;
    }

    pub(in crate::praxis) async fn deliver_event_raw(&self, event: Event) {
        if let Some(status) = agent_status_from_event(&event.msg) {
            self.agent_status.send_replace(status);
        }
        if let Err(e) = self.tx_event.send(event).await {
            debug!("dropping event because channel is closed: {e}");
        }
    }
}

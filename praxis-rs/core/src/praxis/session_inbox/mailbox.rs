use std::sync::Arc;

use praxis_protocol::protocol::InterAgentCommunication;
use tokio::sync::watch;

use crate::praxis::Session;

impl Session {
    pub(crate) fn subscribe_mailbox_seq(&self) -> watch::Receiver<u64> {
        self.mailbox.subscribe()
    }

    pub(crate) fn enqueue_mailbox_communication(&self, communication: InterAgentCommunication) {
        self.mailbox.send(communication);
    }

    pub(crate) async fn receive_inter_agent_communication(
        self: &Arc<Self>,
        sub_id: String,
        communication: InterAgentCommunication,
    ) {
        let trigger_turn = communication.trigger_turn;
        self.enqueue_mailbox_communication(communication);
        if trigger_turn {
            self.maybe_start_turn_for_pending_work_with_sub_id(sub_id)
                .await;
        }
    }

    pub(in crate::praxis) async fn has_trigger_turn_mailbox_items(&self) -> bool {
        self.mailbox_rx.lock().await.has_pending_trigger_turn()
    }
}

use praxis_protocol::models::ResponseInputItem;
use tracing::warn;

use crate::praxis::Session;

const PENDING_INPUT_CHECK_TIMEOUT_MS: u64 = 2_000;

impl Session {
    /// Queue response items to be injected into the next active turn created for this session.
    pub(crate) async fn queue_response_items_for_next_turn(&self, items: Vec<ResponseInputItem>) {
        if items.is_empty() {
            return;
        }

        let mut idle_pending_input = self.idle_pending_input.lock().await;
        idle_pending_input.extend(items);
    }

    async fn take_queued_response_items_for_next_turn(&self) -> Vec<ResponseInputItem> {
        std::mem::take(&mut *self.idle_pending_input.lock().await)
    }

    pub(crate) async fn drain_pending_input_for_started_turn(&self) -> Vec<ResponseInputItem> {
        let mut queued_items = self.take_queued_response_items_for_next_turn().await;
        queued_items.extend(self.get_pending_input().await);
        queued_items
    }

    async fn has_queued_response_items_for_next_turn(&self) -> bool {
        !self.idle_pending_input.lock().await.is_empty()
    }

    pub async fn get_pending_input(&self) -> Vec<ResponseInputItem> {
        let pending_input = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.take_pending_input()
                }
                None => Vec::new(),
            }
        };
        let runtime_command_items = self.claim_runtime_command_input_items().await;
        let mailbox_items = {
            let mut mailbox_rx = self.mailbox_rx.lock().await;
            mailbox_rx
                .drain()
                .into_iter()
                .map(|mail| mail.to_response_input_item())
                .collect::<Vec<_>>()
        };

        let mut combined = Vec::with_capacity(
            pending_input.len() + runtime_command_items.len() + mailbox_items.len(),
        );
        // Priority order matters: explicit input, AgentOS commands, then mailbox notifications.
        combined.extend(pending_input);
        combined.extend(runtime_command_items);
        combined.extend(mailbox_items);
        combined
    }

    pub(crate) async fn has_pending_work_for_idle_turn(&self) -> bool {
        self.has_queued_response_items_for_next_turn().await
            || self
                .services
                .agent_os
                .has_claimable_runtime_command_for_thread(self.conversation_id)
                .await
            || self.has_trigger_turn_mailbox_items().await
    }

    pub async fn has_pending_input(&self) -> bool {
        if self.mailbox_rx.lock().await.has_pending() {
            return true;
        }
        if self
            .services
            .agent_os
            .has_claimable_runtime_command_for_thread(self.conversation_id)
            .await
        {
            return true;
        }
        let active = self.active_turn.lock().await;
        match active.as_ref() {
            Some(at) => {
                let ts = at.turn_state.lock().await;
                ts.has_pending_input()
            }
            None => false,
        }
    }

    pub(crate) async fn has_pending_input_bounded(&self, phase: &'static str) -> bool {
        match tokio::time::timeout(
            std::time::Duration::from_millis(PENDING_INPUT_CHECK_TIMEOUT_MS),
            self.has_pending_input(),
        )
        .await
        {
            Ok(has_pending) => has_pending,
            Err(_) => {
                warn!(
                    thread_id = %self.conversation_id,
                    phase,
                    "timed out checking pending input; assuming none"
                );
                false
            }
        }
    }
}

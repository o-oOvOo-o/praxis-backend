use crate::praxis::Session;
use praxis_protocol::protocol::ThreadHeartbeat;
use std::sync::Arc;

impl Session {
    pub(crate) async fn get_thread_heartbeat(&self) -> anyhow::Result<Option<ThreadHeartbeat>> {
        let state_db = self.require_state_db_for_thread_heartbeats().await?;
        state_db
            .get_thread_heartbeat(self.conversation_id)
            .await
            .map(|heartbeat| heartbeat.map(protocol_heartbeat_from_state))
    }

    pub(crate) async fn user_set_thread_heartbeat(
        self: &Arc<Self>,
        enabled: bool,
        interval_ms: Option<i64>,
        controller: Option<String>,
    ) -> anyhow::Result<Option<ThreadHeartbeat>> {
        let state_db = self.require_state_db_for_thread_heartbeats().await?;
        if enabled && state_db.get_thread_goal(self.conversation_id).await?.is_some() {
            anyhow::bail!("clear the thread goal before enabling heartbeat");
        }
        state_db
            .set_thread_heartbeat(
                self.conversation_id,
                enabled,
                interval_ms,
                controller.as_deref(),
            )
            .await
            .map(|heartbeat| heartbeat.map(protocol_heartbeat_from_state))
    }

    pub(crate) async fn user_clear_thread_heartbeat(self: &Arc<Self>) -> anyhow::Result<bool> {
        let state_db = self.require_state_db_for_thread_heartbeats().await?;
        state_db.delete_thread_heartbeat(self.conversation_id).await
    }

    async fn require_state_db_for_thread_heartbeats(
        &self,
    ) -> anyhow::Result<crate::state_db_bridge::StateDbHandle> {
        self.require_state_db_for_thread_feature("thread heartbeats")
            .await
    }
}

fn protocol_heartbeat_from_state(heartbeat: praxis_state::ThreadHeartbeat) -> ThreadHeartbeat {
    ThreadHeartbeat {
        thread_id: heartbeat.thread_id,
        enabled: heartbeat.enabled,
        interval_ms: heartbeat.interval_ms,
        next_wake_at_ms: heartbeat.next_wake_at.timestamp_millis(),
        last_wake_at_ms: heartbeat
            .last_wake_at
            .map(|last_wake_at| last_wake_at.timestamp_millis()),
        controller: heartbeat.controller,
        created_at_ms: heartbeat.created_at.timestamp_millis(),
        updated_at_ms: heartbeat.updated_at.timestamp_millis(),
    }
}

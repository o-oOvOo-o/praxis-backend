use crate::praxis::Session;
use crate::state_db_bridge::StateDbHandle;
use crate::state_db_bridge::{self as state_db};
use anyhow::Context;
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

    async fn state_db_for_thread_heartbeats(&self) -> anyhow::Result<Option<StateDbHandle>> {
        self.ensure_rollout_materialized().await;
        let state_db = match self.state_db() {
            Some(state_db) => state_db,
            None => {
                let config = self.original_config().await;
                if config.ephemeral {
                    return Ok(None);
                }
                state_db::try_get_state_db(&config).await.with_context(|| {
                    format!(
                        "thread heartbeats require state db at {}",
                        config.sqlite_home.display()
                    )
                })?
            }
        };
        if state_db.get_thread(self.conversation_id).await?.is_none() {
            if let Some(rollout_path) = self.current_rollout_path().await {
                let config = self.original_config().await;
                state_db::reconcile_rollout(
                    Some(state_db.as_ref()),
                    rollout_path.as_path(),
                    config.model_provider_id.as_str(),
                    None,
                    &[],
                    None,
                    None,
                )
                .await;
            }
            if state_db.get_thread(self.conversation_id).await?.is_none() {
                anyhow::bail!("thread heartbeats require materialized thread metadata");
            }
        }
        Ok(Some(state_db))
    }

    async fn require_state_db_for_thread_heartbeats(&self) -> anyhow::Result<StateDbHandle> {
        self.state_db_for_thread_heartbeats().await?.ok_or_else(|| {
            anyhow::anyhow!("thread heartbeats require a persisted thread; this thread is ephemeral")
        })
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

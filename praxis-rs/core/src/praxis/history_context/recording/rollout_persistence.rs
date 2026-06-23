use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::RolloutItem;
use tracing::error;

use crate::praxis::Session;

impl Session {
    pub(in crate::praxis::history_context::recording) async fn persist_rollout_response_items(
        &self,
        items: &[ResponseItem],
    ) {
        let rollout_items: Vec<RolloutItem> = items
            .iter()
            .cloned()
            .map(RolloutItem::ResponseItem)
            .collect();
        self.persist_rollout_items(&rollout_items).await;
    }

    pub(crate) async fn persist_rollout_items(&self, items: &[RolloutItem]) {
        let recorder = {
            let guard = self.services.rollout.lock().await;
            guard.clone()
        };
        if let Some(rec) = recorder
            && let Err(e) = rec.record_items(items).await
        {
            error!("failed to record rollout items: {e:#}");
        }
    }
}

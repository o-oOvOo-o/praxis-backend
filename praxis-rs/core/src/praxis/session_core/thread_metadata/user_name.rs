use std::sync::Arc;

use praxis_protocol::protocol::PraxisErrorInfo;

use crate::praxis::Session;

use super::event::emit_thread_name_updated;
use super::persistence::persist_thread_name;
use super::persistence::rollout_persistence_enabled;
use super::persistence::set_thread_name_in_state;

impl Session {
    pub(crate) async fn set_thread_name_from_user(self: &Arc<Self>, sub_id: String, name: String) {
        let Some(name) = crate::util::normalize_thread_name(&name) else {
            self.raw_event_emitter(sub_id)
                .error(
                    "Thread name cannot be empty.",
                    Some(PraxisErrorInfo::BadRequest),
                )
                .await;
            return;
        };

        if !rollout_persistence_enabled(self).await {
            self.raw_event_emitter(sub_id)
                .error(
                    "Session persistence is disabled; cannot rename thread.",
                    Some(PraxisErrorInfo::Other),
                )
                .await;
            return;
        };

        if let Err(e) = persist_thread_name(self, &name).await {
            self.raw_event_emitter(sub_id)
                .error(
                    format!("Failed to set thread name: {e}"),
                    Some(PraxisErrorInfo::Other),
                )
                .await;
            return;
        }

        set_thread_name_in_state(self, name.clone()).await;
        emit_thread_name_updated(self, sub_id, name).await;
    }
}

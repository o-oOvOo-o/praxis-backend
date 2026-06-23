use std::sync::Arc;

use tracing::warn;

use crate::praxis::Session;

use super::event::emit_thread_name_updated;
use super::persistence::persist_thread_name;
use super::persistence::set_thread_name_in_state;

impl Session {
    /// Persists an automatically generated thread name.
    pub(crate) async fn apply_thread_name(self: &Arc<Self>, name: String) {
        let Some(name) = crate::util::normalize_thread_name(&name) else {
            return;
        };

        if let Err(err) = persist_thread_name(self, &name).await {
            warn!("failed to apply automatic thread name: {err}");
            return;
        }

        set_thread_name_in_state(self, name.clone()).await;
        emit_thread_name_updated(self, self.next_internal_sub_id(), name).await;
    }
}

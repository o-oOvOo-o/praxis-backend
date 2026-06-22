use std::sync::Arc;

use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::ThreadNameUpdatedEvent;
use tracing::warn;

use super::super::Session;

impl Session {
    /// Returns the current thread name, if set.
    pub(crate) async fn thread_name(&self) -> Option<String> {
        let state = self.state.lock().await;
        state.session_configuration.thread_name.clone()
    }

    /// Returns whether thread metadata can be persisted for this session.
    pub(crate) async fn thread_name_persistence_enabled(&self) -> bool {
        let rollout = self.services.rollout.lock().await;
        rollout.is_some() && self.services.state_db.is_some()
    }

    /// Persists an automatically generated thread name.
    pub(crate) async fn apply_thread_name(self: &Arc<Self>, name: String) {
        let Some(name) = crate::util::normalize_thread_name(&name) else {
            return;
        };

        if let Err(err) = praxis_rollout::ThreadNameWriter::new(self.services.state_db.as_deref())
            .write_name(self.conversation_id, &name)
            .await
        {
            warn!("failed to apply automatic thread name: {err}");
            return;
        }

        {
            let mut state = self.state.lock().await;
            state.session_configuration.thread_name = Some(name.clone());
        }

        self.send_event_raw(Event {
            id: self.next_internal_sub_id(),
            msg: EventMsg::ThreadNameUpdated(ThreadNameUpdatedEvent {
                thread_id: self.conversation_id,
                thread_name: Some(name),
            }),
        })
        .await;
    }

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

        let persistence_enabled = {
            let rollout = self.services.rollout.lock().await;
            rollout.is_some()
        };
        if !persistence_enabled {
            self.raw_event_emitter(sub_id)
                .error(
                    "Session persistence is disabled; cannot rename thread.",
                    Some(PraxisErrorInfo::Other),
                )
                .await;
            return;
        };

        if let Err(e) = praxis_rollout::ThreadNameWriter::new(self.services.state_db.as_deref())
            .write_name(self.conversation_id, &name)
            .await
        {
            self.raw_event_emitter(sub_id)
                .error(
                    format!("Failed to set thread name: {e}"),
                    Some(PraxisErrorInfo::Other),
                )
                .await;
            return;
        }

        {
            let mut state = self.state.lock().await;
            state.session_configuration.thread_name = Some(name.clone());
        }

        self.send_event_raw(Event {
            id: sub_id,
            msg: EventMsg::ThreadNameUpdated(ThreadNameUpdatedEvent {
                thread_id: self.conversation_id,
                thread_name: Some(name),
            }),
        })
        .await;
    }
}

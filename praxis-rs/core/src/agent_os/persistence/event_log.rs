use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn record_event(
        &self,
        event_type: &str,
        thread_id: Option<ThreadId>,
        task_id: Option<String>,
        command_id: Option<String>,
        payload: serde_json::Value,
    ) {
        let sequence = self.change_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let entry = EventLedgerEntry {
            sequence,
            event_id: format!("event-{}", Uuid::new_v4()),
            event_type: event_type.to_string(),
            thread_id,
            task_id,
            command_id,
            payload,
            created_at: Utc::now(),
        };
        {
            let mut state = self.state.write().await;
            state.events.push(entry.clone());
            let max_events = AgentOsPolicy::get().max_events_in_memory;
            if state.events.len() > max_events {
                let trim_count = state.events.len() - max_events;
                state.events.drain(0..trim_count);
            }
        }
        if let Some(db) = self.state_db.read().await.clone() {
            let thread_id = entry.thread_id.map(|id| id.to_string());
            if let Err(err) = db
                .record_agent_os_event_json(
                    entry.event_id.as_str(),
                    entry.created_at.timestamp(),
                    entry.event_type.as_str(),
                    thread_id.as_deref(),
                    entry.task_id.as_deref(),
                    entry.command_id.as_deref(),
                    &entry.payload,
                )
                .await
            {
                tracing::warn!("failed to persist AgentOS event: {err}");
            }
        }
        self.change_tx.send_replace(sequence);
    }
}

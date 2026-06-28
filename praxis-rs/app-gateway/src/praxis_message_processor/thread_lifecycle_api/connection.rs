use super::*;

impl PraxisMessageProcessor {
    pub(crate) fn thread_created_receiver(&self) -> broadcast::Receiver<ThreadId> {
        self.thread_manager.subscribe_thread_created()
    }

    pub(crate) async fn connection_initialized(&self, connection_id: ConnectionId) {
        self.thread_state_manager
            .connection_initialized(connection_id)
            .await;
    }

    pub(crate) async fn connection_closed(&mut self, connection_id: ConnectionId) {
        self.command_exec_manager
            .connection_closed(connection_id)
            .await;
        self.thread_state_manager
            .remove_connection(connection_id)
            .await;
    }

    pub(crate) fn subscribe_running_assistant_turn_count(&self) -> watch::Receiver<usize> {
        self.thread_watch_manager.subscribe_running_turn_count()
    }

    /// Best-effort: ensure initialized connections are subscribed to this thread.
    pub(crate) async fn try_attach_thread_listener(
        &mut self,
        thread_id: ThreadId,
        connection_ids: Vec<ConnectionId>,
    ) {
        if let Ok(thread) = self.thread_manager.get_thread(thread_id).await {
            let config_snapshot = thread.config_snapshot().await;
            let loaded_thread =
                build_thread_from_snapshot(thread_id, &config_snapshot, thread.rollout_path());
            self.thread_watch_manager.upsert_thread(loaded_thread).await;
        }

        for connection_id in connection_ids {
            Self::log_listener_attach_result(
                self.ensure_conversation_listener(
                    thread_id,
                    connection_id,
                    /*raw_events_enabled*/ false,
                )
                .await,
                thread_id,
                connection_id,
                "thread",
            );
        }
    }
}

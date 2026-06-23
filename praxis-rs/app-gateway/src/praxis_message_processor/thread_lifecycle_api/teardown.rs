use super::*;

pub(in crate::praxis_message_processor) enum ThreadShutdownResult {
    Complete,
    SubmitFailed,
    TimedOut,
}

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn wait_for_thread_shutdown(
        thread: &Arc<PraxisThread>,
    ) -> ThreadShutdownResult {
        match tokio::time::timeout(Duration::from_secs(10), thread.shutdown_and_wait()).await {
            Ok(Ok(())) => ThreadShutdownResult::Complete,
            Ok(Err(_)) => ThreadShutdownResult::SubmitFailed,
            Err(_) => ThreadShutdownResult::TimedOut,
        }
    }

    pub(in crate::praxis_message_processor) async fn finalize_thread_teardown(
        &mut self,
        thread_id: ThreadId,
    ) {
        self.pending_thread_unloads.lock().await.remove(&thread_id);
        self.outgoing
            .cancel_requests_for_thread(thread_id, /*error*/ None)
            .await;
        self.thread_state_manager
            .remove_thread_state(thread_id)
            .await;
        self.thread_watch_manager
            .remove_thread(&thread_id.to_string())
            .await;
    }
}

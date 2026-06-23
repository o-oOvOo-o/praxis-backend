use super::ThreadShutdownResult;
use super::*;

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn thread_unsubscribe(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadUnsubscribeParams,
    ) {
        let Some(thread_id) = self
            .ensure_thread_id_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };

        let Ok(thread) = self.thread_manager.get_thread(thread_id).await else {
            // Reconcile stale gateway bookkeeping against the core manager source of truth.
            self.finalize_thread_teardown(thread_id).await;
            self.outgoing
                .send_response(
                    request_id,
                    ThreadUnsubscribeResponse {
                        status: ThreadUnsubscribeStatus::NotLoaded,
                    },
                )
                .await;
            return;
        };

        let was_subscribed = self
            .thread_state_manager
            .unsubscribe_connection_from_thread(thread_id, request_id.connection_id)
            .await;
        if !was_subscribed {
            self.outgoing
                .send_response(
                    request_id,
                    ThreadUnsubscribeResponse {
                        status: ThreadUnsubscribeStatus::NotSubscribed,
                    },
                )
                .await;
            return;
        }

        if !self.thread_state_manager.has_subscribers(thread_id).await {
            info!("thread {thread_id} has no subscribers; shutting down");
            self.pending_thread_unloads.lock().await.insert(thread_id);
            self.outgoing
                .cancel_requests_for_thread(thread_id, /*error*/ None)
                .await;
            self.thread_state_manager
                .remove_thread_state(thread_id)
                .await;

            let outgoing = self.outgoing.clone();
            let pending_thread_unloads = self.pending_thread_unloads.clone();
            let thread_manager = self.thread_manager.clone();
            let thread_watch_manager = self.thread_watch_manager.clone();
            tokio::spawn(async move {
                match Self::wait_for_thread_shutdown(&thread).await {
                    ThreadShutdownResult::Complete => {
                        if thread_manager.remove_thread(&thread_id).await.is_none() {
                            info!(
                                "thread {thread_id} was already removed before unsubscribe finalized"
                            );
                            thread_watch_manager
                                .remove_thread(&thread_id.to_string())
                                .await;
                            pending_thread_unloads.lock().await.remove(&thread_id);
                            return;
                        }
                        thread_watch_manager
                            .remove_thread(&thread_id.to_string())
                            .await;
                        let notification = ThreadClosedNotification {
                            thread_id: thread_id.to_string(),
                        };
                        outgoing
                            .send_server_notification(ServerNotification::ThreadClosed(
                                notification,
                            ))
                            .await;
                        pending_thread_unloads.lock().await.remove(&thread_id);
                    }
                    ThreadShutdownResult::SubmitFailed => {
                        pending_thread_unloads.lock().await.remove(&thread_id);
                        warn!("failed to submit Shutdown to thread {thread_id}");
                    }
                    ThreadShutdownResult::TimedOut => {
                        pending_thread_unloads.lock().await.remove(&thread_id);
                        warn!("thread {thread_id} shutdown timed out; leaving thread loaded");
                    }
                }
            });
        }

        self.outgoing
            .send_response(
                request_id,
                ThreadUnsubscribeResponse {
                    status: ThreadUnsubscribeStatus::Unsubscribed,
                },
            )
            .await;
    }
}

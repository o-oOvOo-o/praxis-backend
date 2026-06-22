use super::thread_store_api::ThreadStore;
use super::*;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_control_acquire(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadControlAcquireParams,
    ) {
        let ThreadControlAcquireParams {
            thread_id,
            controller,
            target_rank,
            reason,
        } = params;
        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };
        if let Err(message) = controller.validate_control_access(target_rank) {
            self.send_invalid_request_error(request_id, message).await;
            return;
        }
        if !self.thread_known(thread_uuid).await {
            self.send_invalid_request_error(request_id, format!("thread not found: {thread_uuid}"))
                .await;
            return;
        }

        let control_state = self
            .thread_watch_manager
            .acquire_thread_control(&thread_id, controller, reason)
            .await;
        self.outgoing
            .send_response(request_id, ThreadControlAcquireResponse { control_state })
            .await;
    }

    pub(crate) async fn thread_control_release(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadControlReleaseParams,
    ) {
        let ThreadControlReleaseParams {
            thread_id,
            controller,
        } = params;
        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };
        let current = self
            .thread_watch_manager
            .loaded_runtime_state_for_thread(&thread_id)
            .await
            .control_state;
        if let (Some(expected), Some(current)) = (controller.as_ref(), current.as_ref())
            && &current.controller != expected
        {
            self.send_invalid_request_error(
                request_id,
                format!("thread {thread_uuid} is controlled by a different controller"),
            )
            .await;
            return;
        }

        let previous_control_state = self
            .thread_watch_manager
            .release_thread_control(&thread_id)
            .await;
        self.outgoing
            .send_response(
                request_id,
                ThreadControlReleaseResponse {
                    previous_control_state,
                },
            )
            .await;
    }

    async fn thread_known(&self, thread_id: ThreadId) -> bool {
        if self.thread_manager.get_thread(thread_id).await.is_ok() {
            return true;
        }
        ThreadStore::new(&self.config)
            .thread_exists(thread_id, None)
            .await
            .unwrap_or(false)
    }
}

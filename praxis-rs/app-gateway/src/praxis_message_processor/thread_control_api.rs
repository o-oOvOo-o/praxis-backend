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
        let thread_uuid = match self.parse_thread_id(&thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        if let Err(message) = validate_thread_control_access(&controller, target_rank) {
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
        let thread_uuid = match self.parse_thread_id(&thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let current = self
            .thread_watch_manager
            .loaded_control_state_for_thread(&thread_id)
            .await;
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

    pub(crate) async fn apply_thread_runtime_state(
        &self,
        thread: &mut Thread,
        has_live_in_progress_turn: bool,
    ) {
        let loaded_status = self
            .thread_watch_manager
            .loaded_status_for_thread(&thread.id)
            .await;
        let control_state = self
            .thread_watch_manager
            .loaded_control_state_for_thread(&thread.id)
            .await;
        thread.status = resolve_thread_status(
            loaded_status,
            has_live_in_progress_turn,
            control_state.as_ref(),
        );
        thread.control_state = control_state;
    }

    async fn thread_known(&self, thread_id: ThreadId) -> bool {
        if self.thread_manager.get_thread(thread_id).await.is_ok() {
            return true;
        }
        let directory = praxis_rollout::ThreadDirectory::open(&self.config).await;
        directory
            .thread_exists(thread_id, None)
            .await
            .unwrap_or(false)
    }
}

fn validate_thread_control_access(
    controller: &ThreadController,
    target_rank: Option<u8>,
) -> std::result::Result<(), String> {
    match controller.kind {
        ThreadControllerKind::External => Ok(()),
        ThreadControllerKind::Thread => {
            let Some(rank) = controller.rank else {
                return Err(
                    "agent group thread controllers must include rank 0 or rank 1".to_string(),
                );
            };
            if rank > 1 {
                return Err(
                    "only agent group rank 0 and rank 1 threads can control other threads"
                        .to_string(),
                );
            }
            if let Some(target_rank) = target_rank
                && rank >= target_rank
            {
                return Err(format!(
                    "agent group rank {rank} cannot control rank {target_rank}; same-rank and higher-rank control is forbidden"
                ));
            }
            Ok(())
        }
    }
}

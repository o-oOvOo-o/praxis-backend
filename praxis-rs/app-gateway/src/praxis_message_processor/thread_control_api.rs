use super::thread_store_api::ThreadProjection;
use super::*;
use praxis_protocol::user_input::UserInput as CoreUserInput;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_control_snapshot(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadControlSnapshotParams,
    ) {
        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };
        if !self.thread_known(thread_uuid).await {
            self.send_invalid_request_error(request_id, format!("thread not found: {thread_uuid}"))
                .await;
            return;
        }
        let Some(state_db) = self.thread_control_state_db(request_id.clone()).await else {
            return;
        };
        match self
            .read_thread_control_snapshot(state_db.as_ref(), params.thread_id.as_str())
            .await
        {
            Ok(snapshot) => self.outgoing.send_response(request_id, snapshot).await,
            Err(message) => self.send_internal_error(request_id, message).await,
        }
    }

    pub(crate) async fn thread_control_claim(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadControlClaimParams,
    ) {
        let ThreadControlClaimParams {
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
        let Some(state_db) = self.thread_control_state_db(request_id.clone()).await else {
            return;
        };

        let control_state = self
            .thread_watch_manager
            .acquire_thread_control(&thread_id, controller, reason)
            .await;
        let snapshot = match self
            .read_thread_control_snapshot(state_db.as_ref(), thread_id.as_str())
            .await
        {
            Ok(snapshot) => snapshot,
            Err(message) => {
                self.send_internal_error(request_id, message).await;
                return;
            }
        };
        self.outgoing
            .send_response(
                request_id,
                ThreadControlClaimResponse {
                    control_state,
                    snapshot,
                },
            )
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
        let Some(state_db) = self.thread_control_state_db(request_id.clone()).await else {
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
        let snapshot = match self
            .read_thread_control_snapshot(state_db.as_ref(), thread_id.as_str())
            .await
        {
            Ok(snapshot) => snapshot,
            Err(message) => {
                self.send_internal_error(request_id, message).await;
                return;
            }
        };
        self.outgoing
            .send_response(
                request_id,
                ThreadControlReleaseResponse {
                    previous_control_state,
                    snapshot,
                },
            )
            .await;
    }

    pub(crate) async fn thread_control_queue(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadControlQueueParams,
    ) {
        let ThreadControlQueueParams {
            thread_id,
            controller,
            text,
            queue_id,
        } = params;
        let text = text.trim().to_string();
        if text.is_empty() {
            self.send_invalid_request_error(request_id, "text must not be empty".to_string())
                .await;
            return;
        }
        let queue_id = match queue_id {
            Some(queue_id) => {
                let queue_id = queue_id.trim().to_owned();
                if queue_id.is_empty() || queue_id.len() > 256 {
                    self.send_invalid_request_error(
                        request_id,
                        "queueId must contain 1 to 256 bytes".to_string(),
                    )
                    .await;
                    return;
                }
                Some(queue_id)
            }
            None => None,
        };
        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };
        if !self.thread_known(thread_uuid).await {
            self.send_invalid_request_error(request_id, format!("thread not found: {thread_uuid}"))
                .await;
            return;
        }
        if let Err(message) = self
            .require_active_controller(&thread_id, &controller)
            .await
        {
            self.send_invalid_request_error(request_id, message).await;
            return;
        }
        let Some(state_db) = self.thread_control_state_db(request_id.clone()).await else {
            return;
        };
        let controller_json = match serde_json::to_value(&controller) {
            Ok(value) => value,
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to encode controller: {err}"))
                    .await;
                return;
            }
        };
        let item = match state_db
            .enqueue_thread_control_item(&StateThreadControlQueueCreateParams {
                queue_id,
                target_thread_id: thread_id.clone(),
                controller_json,
                text,
            })
            .await
        {
            Ok(item) => item,
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to enqueue thread control input: {err}"),
                )
                .await;
                return;
            }
        };

        let item = match self
            .try_dispatch_thread_control_queue_item(request_id.clone(), state_db.as_ref(), item)
            .await
        {
            Ok(item) => item,
            Err(message) => {
                self.send_internal_error(request_id, message).await;
                return;
            }
        };
        let item = match api_thread_control_queue_item_from_state(item) {
            Ok(item) => item,
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to decode thread control queue item: {err}"),
                )
                .await;
                return;
            }
        };
        let snapshot = match self
            .read_thread_control_snapshot(state_db.as_ref(), thread_id.as_str())
            .await
        {
            Ok(snapshot) => snapshot,
            Err(message) => {
                self.send_internal_error(request_id, message).await;
                return;
            }
        };
        self.outgoing
            .send_response(request_id, ThreadControlQueueResponse { item, snapshot })
            .await;
    }

    pub(crate) async fn thread_control_queue_cancel(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadControlQueueCancelParams,
    ) {
        let queue_id = params
            .queue_id
            .as_deref()
            .map(str::trim)
            .filter(|queue_id| !queue_id.is_empty());
        let Some(state_db) = self.thread_control_state_db(request_id.clone()).await else {
            return;
        };
        let result = match queue_id {
            Some(queue_id) => {
                state_db
                    .cancel_thread_control_queue_item(&params.thread_id, queue_id)
                    .await
            }
            None => {
                state_db
                    .cancel_latest_thread_control_queue_item(&params.thread_id)
                    .await
            }
        };
        match result {
            Ok(item) => {
                let item = match item
                    .map(api_thread_control_queue_item_from_state)
                    .transpose()
                {
                    Ok(item) => item,
                    Err(err) => {
                        self.send_internal_error(
                            request_id,
                            format!("failed to decode thread control queue item: {err}"),
                        )
                        .await;
                        return;
                    }
                };
                let snapshot = match self
                    .read_thread_control_snapshot(state_db.as_ref(), params.thread_id.as_str())
                    .await
                {
                    Ok(snapshot) => snapshot,
                    Err(message) => {
                        self.send_internal_error(request_id, message).await;
                        return;
                    }
                };
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadControlQueueCancelResponse { item, snapshot },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to cancel thread control queue item: {err}"),
                )
                .await;
            }
        }
    }

    pub(crate) async fn thread_control_queue_flush(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadControlQueueFlushParams,
    ) {
        let Some(state_db) = self.thread_control_state_db(request_id.clone()).await else {
            return;
        };
        match state_db.flush_thread_control_queue(&params.thread_id).await {
            Ok(cancelled) => match api_thread_control_queue_items_from_state(cancelled) {
                Ok(cancelled) => {
                    let snapshot = match self
                        .read_thread_control_snapshot(state_db.as_ref(), params.thread_id.as_str())
                        .await
                    {
                        Ok(snapshot) => snapshot,
                        Err(message) => {
                            self.send_internal_error(request_id, message).await;
                            return;
                        }
                    };
                    self.outgoing
                        .send_response(
                            request_id,
                            ThreadControlQueueFlushResponse {
                                cancelled,
                                snapshot,
                            },
                        )
                        .await;
                }
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!("failed to decode thread control queue items: {err}"),
                    )
                    .await;
                }
            },
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to flush thread control queue: {err}"),
                )
                .await;
            }
        }
    }

    async fn try_dispatch_thread_control_queue_item(
        &self,
        request_id: ConnectionRequestId,
        state_db: &StateRuntime,
        item: StateThreadControlQueueItem,
    ) -> Result<StateThreadControlQueueItem, String> {
        if item.status != StateThreadControlQueueStatus::Queued {
            return Ok(item);
        }
        let thread_id = self
            .parse_thread_id(item.target_thread_id.as_str())
            .map_err(|err| err.message)?;
        let thread = match self.thread_manager.get_thread(thread_id).await {
            Ok(thread) => thread,
            Err(_) => return Ok(item),
        };
        let turn_id = self
            .submit_connection_owned_turn(
                &request_id,
                thread_id,
                thread.as_ref(),
                thread.config_snapshot().await.user_turn_op(
                    vec![CoreUserInput::Text {
                        text: item.text.clone(),
                        text_elements: Vec::new(),
                    }],
                    None,
                ),
                None,
            )
            .await
            .map_err(|err| format!("failed to dispatch thread control input: {err}"))?;
        self.outgoing
            .record_request_turn_id(&request_id, turn_id.as_str())
            .await;
        state_db
            .mark_thread_control_queue_dispatched(item.queue_id.as_str(), turn_id.as_str())
            .await
            .map_err(|err| format!("failed to mark thread control input dispatched: {err}"))?
            .ok_or_else(|| {
                format!(
                    "thread control queue item disappeared before dispatch: {}",
                    item.queue_id
                )
            })
    }

    async fn read_thread_control_snapshot(
        &self,
        state_db: &StateRuntime,
        thread_id: &str,
    ) -> Result<ThreadControlSnapshotResponse, String> {
        let queue = state_db
            .list_thread_control_queue(thread_id, false)
            .await
            .map_err(|err| format!("failed to list thread control queue: {err}"))?;
        let queue = api_thread_control_queue_items_from_state(queue)
            .map_err(|err| format!("failed to decode thread control queue: {err}"))?;
        let control_state = self
            .thread_watch_manager
            .loaded_runtime_state_for_thread(thread_id)
            .await
            .control_state;
        Ok(ThreadControlSnapshotResponse {
            control_state,
            queue,
        })
    }

    async fn require_active_controller(
        &self,
        thread_id: &str,
        controller: &praxis_app_gateway_protocol::ThreadController,
    ) -> Result<(), String> {
        let current = self
            .thread_watch_manager
            .loaded_runtime_state_for_thread(thread_id)
            .await
            .control_state;
        let Some(current) = current else {
            return Err(
                "thread has no active controller; claim control before queueing input".to_string(),
            );
        };
        if &current.controller != controller {
            return Err("thread is controlled by a different controller".to_string());
        }
        Ok(())
    }

    async fn thread_control_state_db(
        &self,
        request_id: ConnectionRequestId,
    ) -> Option<Arc<StateRuntime>> {
        match get_state_db(self.config.as_ref()).await {
            Some(state_db) => Some(state_db),
            None => {
                self.send_internal_error(request_id, "state database is unavailable".to_string())
                    .await;
                None
            }
        }
    }

    async fn thread_known(&self, thread_id: ThreadId) -> bool {
        if self.thread_manager.get_thread(thread_id).await.is_ok() {
            return true;
        }
        ThreadProjection::new(&self.config)
            .thread_exists(thread_id, None)
            .await
            .unwrap_or(false)
    }
}

fn api_thread_control_queue_items_from_state(
    items: Vec<StateThreadControlQueueItem>,
) -> anyhow::Result<Vec<ApiThreadControlQueueItem>> {
    items
        .into_iter()
        .map(api_thread_control_queue_item_from_state)
        .collect()
}

fn api_thread_control_queue_item_from_state(
    item: StateThreadControlQueueItem,
) -> anyhow::Result<ApiThreadControlQueueItem> {
    Ok(ApiThreadControlQueueItem {
        queue_id: item.queue_id,
        target_thread_id: item.target_thread_id,
        controller: serde_json::from_value(item.controller_json)?,
        text: item.text,
        status: api_thread_control_queue_status_from_state(item.status),
        created_at: item.created_at.timestamp_millis(),
        updated_at: item.updated_at.timestamp_millis(),
        dispatched_turn_id: item.dispatched_turn_id,
        error: item.error,
    })
}

fn api_thread_control_queue_status_from_state(
    status: StateThreadControlQueueStatus,
) -> ApiThreadControlQueueStatus {
    match status {
        StateThreadControlQueueStatus::Queued => ApiThreadControlQueueStatus::Queued,
        StateThreadControlQueueStatus::Dispatched => ApiThreadControlQueueStatus::Dispatched,
        StateThreadControlQueueStatus::Completed => ApiThreadControlQueueStatus::Completed,
        StateThreadControlQueueStatus::Cancelled => ApiThreadControlQueueStatus::Cancelled,
        StateThreadControlQueueStatus::Failed => ApiThreadControlQueueStatus::Failed,
    }
}

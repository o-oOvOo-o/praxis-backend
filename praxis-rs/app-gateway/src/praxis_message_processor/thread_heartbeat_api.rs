use super::*;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_heartbeat_get(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadHeartbeatGetParams,
    ) {
        let thread = match self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        {
            Some((_, thread)) => thread,
            None => return,
        };
        match thread.get_thread_heartbeat().await {
            Ok(heartbeat) => {
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadHeartbeatGetResponse {
                            heartbeat: heartbeat.map(Into::into),
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to read thread heartbeat: {err}"),
                )
                .await;
            }
        }
    }

    pub(crate) async fn thread_heartbeat_set(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadHeartbeatSetParams,
    ) {
        let thread_id = params.thread_id.clone();
        let thread = match self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        {
            Some((_, thread)) => thread,
            None => return,
        };
        match thread
            .set_thread_heartbeat_from_user(params.enabled, params.interval_ms, params.controller)
            .await
        {
            Ok(heartbeat) => {
                let heartbeat = heartbeat.map(praxis_app_gateway_protocol::ThreadHeartbeat::from);
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadHeartbeatSetResponse {
                            heartbeat: heartbeat.clone(),
                        },
                    )
                    .await;
                self.broadcast_heartbeat_updated(thread_id, heartbeat).await;
            }
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to set thread heartbeat: {err}"),
                )
                .await;
            }
        }
    }

    pub(crate) async fn thread_heartbeat_clear(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadHeartbeatClearParams,
    ) {
        let thread_id = params.thread_id.clone();
        let thread = match self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        {
            Some((_, thread)) => thread,
            None => return,
        };
        match thread.clear_thread_heartbeat_from_user().await {
            Ok(cleared) => {
                self.outgoing
                    .send_response(request_id, ThreadHeartbeatClearResponse { cleared })
                    .await;
                if cleared {
                    self.broadcast_heartbeat_updated(thread_id, None).await;
                }
            }
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to clear thread heartbeat: {err}"),
                )
                .await;
            }
        }
    }

    pub(crate) async fn broadcast_heartbeat_updated(
        &self,
        thread_id: String,
        heartbeat: Option<praxis_app_gateway_protocol::ThreadHeartbeat>,
    ) {
        self.outgoing
            .send_server_notification(ServerNotification::ThreadHeartbeatUpdated(
                ThreadHeartbeatUpdatedNotification {
                    thread_id,
                    heartbeat,
                },
            ))
            .await;
    }
}

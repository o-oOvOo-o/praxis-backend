use super::*;

impl AgentOs {
    pub(crate) async fn update_worker_request_status(
        &self,
        request_id: &str,
        actor_thread_id: ThreadId,
        status: WorkerRequestStatus,
    ) -> PraxisResult<WorkerRequestRecord> {
        let now = Utc::now();
        let (record, thread_snapshot) = {
            let mut state = self.state.write().await;
            let existing = state
                .worker_requests
                .get(request_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown worker request `{request_id}`"
                    ))
                })?;
            if actor_thread_id != existing.thread_id {
                let requester =
                    state
                        .threads
                        .get(&existing.thread_id)
                        .cloned()
                        .ok_or_else(|| {
                            PraxisErr::UnsupportedOperation(format!(
                                "unknown AgentOS request thread `{}`",
                                existing.thread_id
                            ))
                        })?;
                let actor = state
                    .threads
                    .get(&actor_thread_id)
                    .cloned()
                    .ok_or_else(|| {
                        PraxisErr::UnsupportedOperation(format!(
                            "unknown AgentOS actor thread `{actor_thread_id}`"
                        ))
                    })?;
                if actor.coordination_scope != requester.coordination_scope {
                    return Err(PraxisErr::UnsupportedOperation(
                        "worker request status can only be updated by owner or active coordinator"
                            .to_string(),
                    ));
                }
                Self::claim_or_renew_active_coordinator_locked(
                    &mut state,
                    &actor,
                    now,
                    Some("resolve worker requests"),
                )?;
            }
            let request = state.worker_requests.get_mut(request_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown worker request `{request_id}`"))
            })?;
            request.status = status;
            request.updated_at = now;
            let record = request.clone();
            let thread_snapshot = if record.blocking && status != WorkerRequestStatus::Pending {
                state.threads.get_mut(&record.thread_id).map(|thread| {
                    if matches!(
                        thread.state,
                        ThreadRuntimeState::WaitingForLease
                            | ThreadRuntimeState::WaitingForCoordinator
                    ) {
                        thread.state = ThreadRuntimeState::Idle;
                    }
                    thread.heartbeat_at = now;
                    thread.clone()
                })
            } else {
                None
            };
            (record, thread_snapshot)
        };

        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        self.persist_worker_request_snapshot(&record).await;
        self.record_event(
            "worker_request_status_updated",
            Some(actor_thread_id),
            record.task_id.clone(),
            None,
            json!({
                "request_id": &record.request_id,
                "request_thread_id": record.thread_id.to_string(),
                "request_type": &record.request_type,
                "status": format!("{:?}", record.status),
            }),
        )
        .await;
        Ok(record)
    }
}

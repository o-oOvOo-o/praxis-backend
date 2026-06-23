use super::*;

impl AgentOs {
    pub(crate) async fn submit_worker_request(
        &self,
        request: WorkerRequestCreateRequest,
    ) -> PraxisResult<WorkerRequestRecord> {
        let request_type = request.request_type.trim().to_string();
        if request_type.is_empty() {
            return Err(PraxisErr::UnsupportedOperation(
                "worker request_type cannot be empty".to_string(),
            ));
        }
        let reason = request.reason.trim().to_string();
        if reason.is_empty() {
            return Err(PraxisErr::UnsupportedOperation(
                "worker request reason cannot be empty".to_string(),
            ));
        }

        let now = Utc::now();
        let request_id = format!("worker-request-{}", Uuid::new_v4());
        let (record, thread_snapshot) = {
            let mut state = self.state.write().await;
            let thread = state.threads.get_mut(&request.thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS thread `{}`",
                    request.thread_id
                ))
            })?;
            let task_id = thread.current_task_id.clone();
            if request.blocking {
                thread.state = if request_type.eq_ignore_ascii_case("BlockedByLease") {
                    ThreadRuntimeState::WaitingForLease
                } else {
                    ThreadRuntimeState::WaitingForCoordinator
                };
                thread.heartbeat_at = now;
            }
            let thread_snapshot = thread.clone();
            let record = WorkerRequestRecord {
                request_id: request_id.clone(),
                request_type,
                thread_id: request.thread_id,
                task_id,
                blocking: request.blocking,
                status: WorkerRequestStatus::Pending,
                reason,
                requested_resource: request.requested_resource,
                artifact_refs: request.artifact_refs,
                created_at: now,
                updated_at: now,
            };
            state.worker_requests.insert(request_id, record.clone());
            (record, thread_snapshot)
        };

        self.persist_thread_snapshot(&thread_snapshot).await;
        self.persist_worker_request_snapshot(&record).await;
        self.record_event(
            "worker_request_submitted",
            Some(record.thread_id),
            record.task_id.clone(),
            None,
            json!({
                "request_id": &record.request_id,
                "request_type": &record.request_type,
                "blocking": record.blocking,
                "status": format!("{:?}", record.status),
                "reason": &record.reason,
                "requested_resource": &record.requested_resource,
                "artifact_refs": &record.artifact_refs,
            }),
        )
        .await;

        Ok(record)
    }
}

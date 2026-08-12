use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn acquire_required_leases(
        &self,
        thread_id: ThreadId,
        task_id: &str,
        priority: i32,
        requirements: &[ResourceRequirement],
    ) -> PraxisResult<Vec<String>> {
        self.acquire_required_leases_with_timeout(
            thread_id,
            task_id,
            priority,
            requirements,
            AgentOsPolicy::get().lease_wait_timeout(),
        )
        .await
    }

    pub(in crate::agent_os) async fn acquire_required_leases_with_timeout(
        &self,
        thread_id: ThreadId,
        task_id: &str,
        priority: i32,
        requirements: &[ResourceRequirement],
        wait_timeout: std::time::Duration,
    ) -> PraxisResult<Vec<String>> {
        let mut seen = HashSet::new();
        let planned_requirements = requirements
            .iter()
            .filter(|requirement| seen.insert(requirement.key()))
            .cloned()
            .collect::<Vec<_>>();

        let mut wait_started_at = None;
        let wait_deadline = tokio::time::Instant::now() + wait_timeout;
        let (acquired, snapshots) = loop {
            // Register before checking the lease table so a release between the check and await
            // cannot be lost. Cancellation safely drops this waiter through the outer tool runtime.
            let released = self.lease_released.notified();
            tokio::pin!(released);
            released.as_mut().enable();

            let mut state = self.state.write().await;
            let conflict = planned_requirements.iter().find_map(|requirement| {
                self.lease_conflict_owner_locked(&state, requirement)
                    .map(|owner| (requirement.key(), owner))
            });
            if let Some((key, owner)) = conflict {
                tracing::debug!(
                    thread_id = %thread_id,
                    task_id,
                    resource = %key,
                    lease_owner = %owner,
                    "waiting for conflicting resource lease"
                );
                drop(state);
                if owner == thread_id.to_string() {
                    self.record_event(
                        "lease_self_conflict",
                        Some(thread_id),
                        Some(task_id.to_string()),
                        None,
                        json!({
                            "resource": &key,
                            "lease_owner": &owner,
                        }),
                    )
                    .await;
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "resource lease self-conflict for `{key}`: this thread already owns the conflicting lease; wait for or stop its earlier tool before retrying"
                    )));
                }
                if wait_started_at.is_none() {
                    let started_at = Utc::now();
                    wait_started_at = Some(started_at);
                    self.mark_thread_state(thread_id, ThreadRuntimeState::WaitingForLease)
                        .await;
                    self.record_event(
                        "lease_wait_started",
                        Some(thread_id),
                        Some(task_id.to_string()),
                        None,
                        json!({
                            "resource": &key,
                            "lease_owner": &owner,
                            "queued_at": started_at,
                        }),
                    )
                    .await;
                }
                if tokio::time::timeout_at(wait_deadline, released)
                    .await
                    .is_err()
                {
                    self.mark_thread_state(thread_id, ThreadRuntimeState::Running)
                        .await;
                    self.record_event(
                        "lease_wait_timed_out",
                        Some(thread_id),
                        Some(task_id.to_string()),
                        None,
                        json!({
                            "resource": &key,
                            "lease_owner": &owner,
                            "waited_ms": wait_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
                        }),
                    )
                    .await;
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "timed out after {} ms waiting for resource lease `{key}` held by thread `{owner}`; retry after the owner finishes or stop the stale owner",
                        wait_timeout.as_millis()
                    )));
                }
                continue;
            }

            let now = Utc::now();
            let mut acquired = Vec::new();
            let mut snapshots = Vec::new();
            state.fencing_counter = state.fencing_counter.saturating_add(1);
            let fencing_token = state.fencing_counter;
            for requirement in &planned_requirements {
                let key = requirement.key();
                let lease = ResourceLease {
                    lease_id: format!("lease-{}", Uuid::new_v4()),
                    resource_type: requirement.resource_type().to_string(),
                    scope: key,
                    mode: requirement.mode(),
                    owner_thread_id: thread_id,
                    task_id: task_id.to_string(),
                    priority,
                    fencing_token,
                    created_at: now,
                    expires_at: Some(now + AgentOsPolicy::get().lease_ttl()),
                    revocable: true,
                    metadata: json!({}),
                    command_id: None,
                    process_id: None,
                    runtime_owner_id: None,
                };
                acquired.push(lease.lease_id.clone());
                snapshots.push(lease.clone());
                state.leases.insert(lease.lease_id.clone(), lease);
            }
            break (acquired, snapshots);
        };

        if let Some(started_at) = wait_started_at {
            self.mark_thread_state(thread_id, ThreadRuntimeState::Running)
                .await;
            self.record_event(
                "lease_wait_finished",
                Some(thread_id),
                Some(task_id.to_string()),
                None,
                json!({
                    "waited_ms": Utc::now()
                        .signed_duration_since(started_at)
                        .num_milliseconds()
                        .max(0),
                }),
            )
            .await;
        }

        for lease in snapshots {
            self.persist_lease_snapshot(&lease).await;
            self.record_event(
                "lease_acquired",
                Some(thread_id),
                Some(task_id.to_string()),
                None,
                json!({
                    "lease_id": lease.lease_id,
                    "resource_type": lease.resource_type,
                    "scope": lease.scope,
                    "mode": lease.mode.as_str(),
                }),
            )
            .await;
        }
        Ok(acquired)
    }

    pub(in crate::agent_os) async fn release_leases(&self, lease_ids: &[String]) {
        let mut released = Vec::new();
        {
            let mut state = self.state.write().await;
            for lease_id in lease_ids {
                if let Some(lease) = state.leases.remove(lease_id) {
                    released.push(lease);
                }
            }
        }
        if !released.is_empty() {
            self.lease_released.notify_waiters();
        }
        for lease in released {
            self.record_event(
                "lease_released",
                Some(lease.owner_thread_id),
                Some(lease.task_id),
                None,
                json!({
                    "lease_id": lease.lease_id,
                    "resource_type": lease.resource_type,
                    "scope": lease.scope,
                }),
            )
            .await;
        }
    }
}

use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn acquire_required_leases(
        &self,
        thread_id: ThreadId,
        task_id: &str,
        priority: i32,
        requirements: &[ResourceRequirement],
    ) -> PraxisResult<Vec<String>> {
        let mut seen = HashSet::new();
        let planned_requirements = requirements
            .iter()
            .filter(|requirement| seen.insert(requirement.key()))
            .cloned()
            .collect::<Vec<_>>();

        let mut wait_started_at = None;
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
                            "resource": key,
                            "lease_owner": owner,
                            "queued_at": started_at,
                        }),
                    )
                    .await;
                }
                released.await;
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

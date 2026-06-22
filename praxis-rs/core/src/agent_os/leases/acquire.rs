use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn acquire_required_leases(
        &self,
        thread_id: ThreadId,
        task_id: &str,
        priority: i32,
        requirements: &[ResourceRequirement],
    ) -> PraxisResult<Vec<String>> {
        let now = Utc::now();
        let mut seen = HashSet::new();
        let planned_requirements = requirements
            .iter()
            .filter(|requirement| seen.insert(requirement.key()))
            .cloned()
            .collect::<Vec<_>>();
        let mut acquired = Vec::new();
        let mut snapshots = Vec::new();
        {
            let mut state = self.state.write().await;
            for requirement in &planned_requirements {
                if let Some(owner) = self.lease_conflict_owner_locked(&state, requirement) {
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "resource lease `{}` is held by {owner}",
                        requirement.key()
                    )));
                }
            }
            state.fencing_counter = state.fencing_counter.saturating_add(1);
            let fencing_token = state.fencing_counter;
            for requirement in planned_requirements {
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

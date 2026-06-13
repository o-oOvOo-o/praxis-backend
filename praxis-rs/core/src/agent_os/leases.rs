use super::*;

impl AgentOsRuntime {
    pub(super) async fn acquire_required_leases(
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

    pub(super) async fn release_leases(&self, lease_ids: &[String]) {
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

    pub(super) async fn expire_leases(&self) {
        let now = Utc::now();
        let mut expired = Vec::new();
        {
            let mut state = self.state.write().await;
            let ids: Vec<String> = state
                .leases
                .iter()
                .filter_map(|(lease_id, lease)| {
                    lease
                        .expires_at
                        .is_some_and(|expires_at| expires_at <= now)
                        .then(|| lease_id.clone())
                })
                .collect();
            for lease_id in ids {
                if let Some(lease) = state.leases.remove(&lease_id) {
                    expired.push(lease);
                }
            }
        }
        let mut cleanup_processes = HashSet::new();
        let mut finish_commands = HashSet::new();
        for lease in expired {
            if let Some(process_id) = lease.process_id {
                cleanup_processes.insert((process_id, lease.runtime_owner_id.clone()));
            }
            if let Some(command_id) = lease.command_id.clone() {
                finish_commands.insert(command_id);
            }
            self.record_event(
                "lease_expired",
                Some(lease.owner_thread_id),
                Some(lease.task_id),
                lease.command_id.clone(),
                json!({
                    "lease_id": lease.lease_id,
                    "scope": lease.scope,
                    "process_id": lease.process_id,
                    "runtime_owner_id": lease.runtime_owner_id,
                    "requires_process_cleanup": lease.process_id.is_some(),
                }),
            )
            .await;
        }
        for (process_id, runtime_owner_id) in cleanup_processes {
            self.mark_process_status(
                process_id,
                runtime_owner_id.as_deref(),
                ManagedProcessStatus::Cleaning,
            )
            .await;
            let cleaned = self
                .cleanup_process(process_id, runtime_owner_id.as_deref())
                .await;
            if cleaned {
                self.mark_process_finished(process_id, runtime_owner_id.as_deref())
                    .await;
            }
            self.record_event(
                "lease_process_cleanup",
                None,
                None,
                None,
                json!({
                    "process_id": process_id,
                    "runtime_owner_id": runtime_owner_id,
                    "cleaned": cleaned,
                }),
            )
            .await;
        }
        for command_id in finish_commands {
            let _ = self
                .finish_managed_command(
                    command_id.as_str(),
                    Some(-1),
                    b"command terminated because AgentOS lease expired",
                    /*release_leases*/ false,
                )
                .await;
        }
    }

    pub(super) async fn expire_tickets(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            let active_ticket_ids = state
                .commands
                .values()
                .filter(|command| command.ended_at.is_none())
                .map(|command| command.ticket_id.clone())
                .collect::<HashSet<_>>();
            let ids = state
                .tickets
                .iter()
                .filter(|(_, ticket)| ticket.expires_at <= now)
                .filter(|(ticket_id, _)| !active_ticket_ids.contains(ticket_id.as_str()))
                .map(|(ticket_id, _)| ticket_id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|ticket_id| state.tickets.remove(ticket_id.as_str()))
                .collect::<Vec<_>>()
        };
        for ticket in expired {
            self.release_leases(&ticket.lease_ids).await;
            self.persist_revoked_ticket_snapshot(&ticket, "ticket expired before completion")
                .await;
            self.record_event(
                "ticket_expired",
                Some(ticket.thread_id),
                Some(ticket.task_id.clone()),
                None,
                json!({
                    "ticket_id": &ticket.ticket_id,
                    "intent_plan_id": &ticket.intent_plan_id,
                    "expires_at": ticket.expires_at.to_rfc3339(),
                }),
            )
            .await;
        }
    }

    pub(super) async fn cleanup_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> bool {
        let (runtime_kind, process_owner_id) = {
            let state = self.state.read().await;
            let process_key = process_registry_key(process_id, runtime_owner_id);
            state
                .processes
                .get(process_key.as_str())
                .map(|process| {
                    (
                        Some(process.runtime_kind.clone()),
                        process.runtime_owner_id.clone(),
                    )
                })
                .unwrap_or((None, runtime_owner_id.map(str::to_string)))
        };

        if let (Some(runtime_kind), Some(process_owner_id)) =
            (runtime_kind.as_deref(), process_owner_id.as_deref())
        {
            let exact_key = cleaner_registry_key(runtime_kind, process_owner_id);
            let cleaner = self
                .process_cleaners_by_owner
                .read()
                .await
                .get(exact_key.as_str())
                .cloned();
            if let Some(cleaner) = cleaner {
                if cleaner.cleanup_agent_os_process(process_id).await {
                    return true;
                }
            }
        }

        let cleaners = {
            let cleaners_by_kind = self.process_cleaners.read().await;
            let mut selected = Vec::new();

            // If the process record has an owning backend id, process ids are
            // backend-local. Do not fan out to every same-kind cleaner: that can
            // kill the wrong backend's process when two sessions reuse the same
            // numeric id. A generic cleaner may still handle host-global process
            // ids, but owner-scoped processes require exact routing.
            if process_owner_id.is_none() {
                if let Some(runtime_kind) = runtime_kind.as_deref() {
                    if let Some(cleaners) = cleaners_by_kind.get(runtime_kind) {
                        selected.extend(cleaners.iter().cloned());
                    }
                }
            }
            if let Some(cleaners) = cleaners_by_kind.get(process_runtime_kind::GENERIC) {
                selected.extend(cleaners.iter().cloned());
            }
            if selected.is_empty() && process_owner_id.is_none() {
                selected.extend(cleaners_by_kind.values().flatten().cloned());
            }
            selected
        };
        for cleaner in cleaners {
            if cleaner.cleanup_agent_os_process(process_id).await {
                return true;
            }
        }
        false
    }

    pub(super) fn lease_conflict_owner_locked(
        &self,
        state: &AgentOsState,
        requirement: &ResourceRequirement,
    ) -> Option<String> {
        let key = requirement.key();
        let mode = requirement.mode();
        match mode {
            LeaseMode::Advisory | LeaseMode::Shared => None,
            LeaseMode::Capacity => {
                let capacity = capacity_for_requirement(requirement);
                let active = state
                    .leases
                    .values()
                    .filter(|lease| lease.scope == key)
                    .filter(|lease| {
                        lease
                            .expires_at
                            .is_none_or(|expires_at| expires_at > Utc::now())
                    })
                    .collect::<Vec<_>>();
                (active.len() >= capacity).then(|| {
                    active
                        .first()
                        .map(|lease| lease.owner_thread_id.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                })
            }
            LeaseMode::Exclusive => state
                .leases
                .values()
                .find(|lease| {
                    lease.scope == key
                        && lease
                            .expires_at
                            .is_none_or(|expires_at| expires_at > Utc::now())
                })
                .map(|lease| lease.owner_thread_id.to_string()),
        }
    }
}

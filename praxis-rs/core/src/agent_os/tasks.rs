use super::*;

impl AgentOsRuntime {
    pub(crate) async fn register_thread(
        &self,
        registration: ThreadRegistration,
    ) -> PraxisResult<()> {
        let now = Utc::now();
        let entry = ThreadRegistryEntry {
            thread_id: registration.thread_id,
            coordination_scope: registration.coordination_scope,
            rank: registration.rank,
            profile_id: registration.profile_id,
            cwd: registration.cwd,
            repo_id: registration.repo_id,
            branch: registration.branch,
            worktree: registration.worktree,
            current_task_id: None,
            current_command_id: None,
            state: ThreadRuntimeState::Idle,
            heartbeat_at: now,
            priority: registration.priority,
            created_at: now,
        };

        {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            if entry.rank == COORDINATOR_RANK {
                let coordinator_count = state
                    .threads
                    .values()
                    .filter(|thread| thread.rank == COORDINATOR_RANK)
                    .filter(|thread| thread.coordination_scope == entry.coordination_scope)
                    .filter(|thread| thread.thread_id != entry.thread_id)
                    .filter(|thread| {
                        !matches!(
                            thread.state,
                            ThreadRuntimeState::Stopped
                                | ThreadRuntimeState::Failed
                                | ThreadRuntimeState::Completed
                        )
                    })
                    .count();
                if coordinator_count >= MAX_COORDINATORS {
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "rank-0 coordinator limit reached for scope `{}`",
                        entry.coordination_scope
                    )));
                }
                let active_state = state
                    .active_coordinators
                    .get(entry.coordination_scope.as_str())
                    .map(|active| (active.owner_thread_id, active.expires_at));
                match active_state {
                    Some((owner, expires_at)) if owner == entry.thread_id && expires_at > now => {
                        if let Some(active) = state
                            .active_coordinators
                            .get_mut(entry.coordination_scope.as_str())
                        {
                            active.expires_at = now + AgentOsPolicy::get().lease_ttl();
                        }
                    }
                    Some((_owner, expires_at)) if expires_at > now => {
                        // Another live rank-0 already owns dispatch for this scope.
                        // This coordinator registers as council/advisor, not active scheduler.
                    }
                    _ => {
                        state.coordinator_epoch = state.coordinator_epoch.saturating_add(1);
                        state.fencing_counter = state.fencing_counter.saturating_add(1);
                        let epoch = state.coordinator_epoch;
                        let fencing_token = state.fencing_counter;
                        state.active_coordinators.insert(
                            entry.coordination_scope.clone(),
                            ActiveCoordinatorLease {
                                coordination_scope: entry.coordination_scope.clone(),
                                owner_thread_id: entry.thread_id,
                                epoch,
                                fencing_token,
                                expires_at: now + AgentOsPolicy::get().lease_ttl(),
                            },
                        );
                    }
                }
            }
            state.threads.insert(entry.thread_id, entry.clone());
        }

        self.persist_thread_snapshot(&entry).await;
        self.record_event(
            "thread_registered",
            Some(entry.thread_id),
            None,
            None,
            json!({
                "coordination_scope": entry.coordination_scope,
                "rank": entry.rank,
                "profile_id": entry.profile_id,
                "cwd": entry.cwd,
            }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn ensure_bootstrap_task(
        &self,
        thread_id: ThreadId,
        objective: impl Into<String>,
        scope: Vec<String>,
    ) -> PraxisResult<String> {
        if let Some(task_id) = self
            .state
            .read()
            .await
            .threads
            .get(&thread_id)
            .and_then(|thread| thread.current_task_id.clone())
        {
            return Ok(task_id);
        }
        let task = self
            .create_task(TaskCreateRequest {
                objective: objective.into(),
                scope,
                constraints: Vec::new(),
                acceptance_criteria: Vec::new(),
                artifact_refs: Vec::new(),
                priority: 0,
                assigned_thread_id: Some(thread_id),
                required_capabilities: Vec::new(),
                required_resources: Vec::new(),
                token_budget: None,
                exploratory: true,
                created_by: thread_id,
            })
            .await?;
        self.assign_task(task.as_str(), thread_id).await?;
        Ok(task)
    }

    pub(crate) async fn create_task(&self, request: TaskCreateRequest) -> PraxisResult<String> {
        let now = Utc::now();
        let task_id = format!("task-{}", Uuid::new_v4());
        let assigned_thread_id = request.assigned_thread_id;
        if assigned_thread_id.is_some() && request.scope.is_empty() && !request.exploratory {
            return Err(PraxisErr::UnsupportedOperation(
                "assigned AgentOS tasks require non-empty scope unless exploratory=true"
                    .to_string(),
            ));
        }
        let task = TaskRecord {
            task_id: task_id.clone(),
            objective: request.objective,
            scope: request.scope,
            constraints: request.constraints,
            acceptance_criteria: request.acceptance_criteria,
            artifact_refs: request.artifact_refs,
            status: if assigned_thread_id.is_some() {
                TaskStatus::Assigned
            } else {
                TaskStatus::Pending
            },
            priority: request.priority,
            assigned_thread_id: assigned_thread_id.clone(),
            required_capabilities: request.required_capabilities,
            required_resources: request.required_resources,
            token_budget: request.token_budget,
            artifact_read_bytes: 0,
            exploratory: request.exploratory,
            created_by: request.created_by,
            created_at: now,
            updated_at: now,
        };

        {
            let mut state = self.state.write().await;
            state.tasks.insert(task_id.clone(), task.clone());
        }
        self.persist_task_snapshot(&task).await;
        self.record_event(
            "task_created",
            assigned_thread_id,
            Some(task_id.clone()),
            None,
            json!({
                "objective": task.objective,
                "scope": task.scope,
                "constraints": task.constraints,
                "acceptance_criteria": task.acceptance_criteria,
                "artifact_refs": task.artifact_refs,
                "priority": task.priority,
                "exploratory": task.exploratory,
            }),
        )
        .await;
        Ok(task_id)
    }

    pub(crate) async fn assign_task(&self, task_id: &str, thread_id: ThreadId) -> PraxisResult<()> {
        let (thread_snapshot, task_snapshot) = {
            let mut state = self.state.write().await;
            let task = state.tasks.get_mut(task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown task `{task_id}`"))
            })?;
            task.assigned_thread_id = Some(thread_id);
            task.status = TaskStatus::Assigned;
            task.updated_at = Utc::now();
            let task_snapshot = task.clone();
            let thread = state.threads.get_mut(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            thread.current_task_id = Some(task_id.to_string());
            thread.state = ThreadRuntimeState::Assigned;
            thread.heartbeat_at = Utc::now();
            (thread.clone(), task_snapshot)
        };

        self.persist_thread_snapshot(&thread_snapshot).await;
        self.persist_task_snapshot(&task_snapshot).await;
        self.record_event(
            "task_assigned",
            Some(thread_id),
            Some(task_id.to_string()),
            None,
            json!({ "thread_id": thread_id.to_string() }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn heartbeat_thread(&self, thread_id: ThreadId) -> PraxisResult<()> {
        let now = Utc::now();
        let thread_snapshot = {
            let mut state = self.state.write().await;
            let thread = state.threads.get_mut(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            thread.heartbeat_at = now;
            let snapshot = thread.clone();
            if snapshot.rank == COORDINATOR_RANK
                && let Err(err) =
                    Self::claim_or_renew_active_coordinator_locked(&mut state, &snapshot, now, None)
            {
                tracing::debug!(%err, %thread_id, "rank-0 heartbeat did not renew coordinator lease");
            }
            snapshot
        };
        self.persist_thread_snapshot(&thread_snapshot).await;
        self.note_runtime_command_activity(thread_id, RuntimeCommandActivity::WorkerHeartbeat)
            .await;
        Ok(())
    }

    pub(crate) async fn ensure_inter_thread_message_allowed(
        &self,
        from_thread_id: ThreadId,
        to_thread_id: ThreadId,
        require_active_dispatcher: bool,
    ) -> PraxisResult<()> {
        let mut state = self.state.write().await;
        let sender = state.threads.get(&from_thread_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS sender thread `{from_thread_id}`"
            ))
        })?;
        let receiver = state.threads.get(&to_thread_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS receiver thread `{to_thread_id}`"
            ))
        })?;
        if sender.rank != COORDINATOR_RANK {
            return Err(PraxisErr::UnsupportedOperation(
                "worker-to-worker natural-language messaging is disabled by AgentOS; submit artifacts, status, or structured requests instead".to_string(),
            ));
        }
        if receiver.coordination_scope != sender.coordination_scope {
            return Err(PraxisErr::UnsupportedOperation(
                "inter-thread messaging cannot cross coordination scopes".to_string(),
            ));
        }
        if require_active_dispatcher {
            Self::claim_or_renew_active_coordinator_locked(
                &mut state,
                &sender,
                Utc::now(),
                Some("dispatch tasks"),
            )?;
        }
        Ok(())
    }

    pub(super) async fn mark_thread_state(
        &self,
        thread_id: ThreadId,
        state_value: ThreadRuntimeState,
    ) {
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(thread) = state.threads.get_mut(&thread_id) else {
                return;
            };
            thread.state = state_value;
            thread.heartbeat_at = Utc::now();
            thread.clone()
        };
        self.persist_thread_snapshot(&snapshot).await;
    }
}

use super::*;

impl AgentOs {
    pub(super) async fn note_runtime_command_activity(
        &self,
        thread_id: ThreadId,
        activity: RuntimeCommandActivity,
    ) -> Vec<RuntimeCommandRecord> {
        let now = Utc::now();
        let changed = {
            let mut state = self.state.write().await;
            let current_task_id = state
                .threads
                .get_mut(&thread_id)
                .map(|thread| {
                    thread.heartbeat_at = now;
                    thread.current_task_id.clone()
                })
                .unwrap_or_default();
            let mut changed = Vec::new();
            for command in state.runtime_commands.values_mut() {
                if command.to_thread_id != thread_id {
                    continue;
                }
                if command.expires_at <= now {
                    if matches!(
                        command.status,
                        RuntimeCommandStatus::Pending
                            | RuntimeCommandStatus::Acked
                            | RuntimeCommandStatus::Executing
                    ) {
                        command.status = RuntimeCommandStatus::Expired;
                        command.updated_at = now;
                        changed.push(command.clone());
                    }
                    continue;
                }
                let mut command_changed = false;
                if matches!(
                    command.status,
                    RuntimeCommandStatus::Pending
                        | RuntimeCommandStatus::Acked
                        | RuntimeCommandStatus::Executing
                ) {
                    command.expires_at = now + AgentOsPolicy::get().ticket_ttl();
                    command.updated_at = now;
                    command_changed = true;
                }
                match (activity, command.status, command.command_type) {
                    (_, RuntimeCommandStatus::Pending, _) => {
                        command.status = RuntimeCommandStatus::Acked;
                        command_changed = true;
                    }
                    (
                        RuntimeCommandActivity::WorkerStartedCommand,
                        RuntimeCommandStatus::Acked,
                        RuntimeCommandType::AssignTask,
                    ) if command.task_id == current_task_id => {
                        command.status = RuntimeCommandStatus::Executing;
                        command_changed = true;
                    }
                    _ => {}
                }
                if command_changed {
                    changed.push(command.clone());
                }
            }
            changed
        };
        for command in &changed {
            self.persist_runtime_command_snapshot(command).await;
        }
        if !changed.is_empty() {
            self.record_event(
                "runtime_command_activity_synced",
                Some(thread_id),
                None,
                None,
                json!({
                    "activity": format!("{:?}", activity),
                    "changed_commands": changed
                        .iter()
                        .map(|command| json!({
                            "command_id": &command.command_id,
                            "command_type": command.command_type.as_str(),
                            "status": format!("{:?}", command.status),
                            "task_id": &command.task_id,
                        }))
                        .collect::<Vec<_>>(),
                }),
            )
            .await;
        }
        changed
    }

    pub(crate) async fn complete_active_runtime_command_for_thread(
        &self,
        thread_id: ThreadId,
        succeeded: bool,
        reason: impl Into<String>,
    ) -> PraxisResult<Option<RuntimeCommandRecord>> {
        let reason = reason.into();
        let (command_id, task_id, blocked, task_already_failed) = {
            let state = self.state.read().await;
            let candidate = state
                .runtime_commands
                .values()
                .filter(|command| {
                    command.to_thread_id == thread_id
                        && command.command_type == RuntimeCommandType::AssignTask
                        && matches!(
                            command.status,
                            RuntimeCommandStatus::Pending
                                | RuntimeCommandStatus::Acked
                                | RuntimeCommandStatus::Executing
                        )
                })
                .max_by_key(|command| {
                    let status_rank = match command.status {
                        RuntimeCommandStatus::Executing => 2,
                        RuntimeCommandStatus::Acked => 1,
                        RuntimeCommandStatus::Pending => 0,
                        RuntimeCommandStatus::Completed
                        | RuntimeCommandStatus::Failed
                        | RuntimeCommandStatus::Expired
                        | RuntimeCommandStatus::Rejected => -1,
                    };
                    (status_rank, command.updated_at.timestamp_millis())
                })
                .cloned();
            let Some(command) = candidate else {
                return Ok(None);
            };
            let blocked = command.task_id.as_ref().is_some_and(|task_id| {
                state.worker_requests.values().any(|request| {
                    request.thread_id == thread_id
                        && request.task_id.as_deref() == Some(task_id.as_str())
                        && request.blocking
                        && matches!(request.status, WorkerRequestStatus::Pending)
                })
            });
            let task_already_failed = command.task_id.as_ref().is_some_and(|task_id| {
                state.tasks.get(task_id).is_some_and(|task| {
                    matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled)
                })
            });
            (
                command.command_id,
                command.task_id.clone(),
                blocked,
                task_already_failed,
            )
        };

        if succeeded && !task_already_failed && blocked {
            self.record_event(
                "runtime_command_completion_deferred",
                Some(thread_id),
                task_id,
                Some(command_id),
                json!({
                    "reason": reason,
                    "deferred_because": "pending_blocking_worker_request",
                }),
            )
            .await;
            return Ok(None);
        }

        let status = if succeeded && !task_already_failed {
            RuntimeCommandStatus::Completed
        } else {
            RuntimeCommandStatus::Failed
        };
        let command = self
            .update_runtime_command_status(command_id.as_str(), thread_id, status)
            .await?;
        self.record_event(
            "runtime_command_lifecycle_completed",
            Some(thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "status": format!("{:?}", command.status),
                "reason": reason,
                "task_already_failed": task_already_failed,
            }),
        )
        .await;
        Ok(Some(command))
    }

    pub(crate) async fn cleanup_thread_resources_after_abort(
        &self,
        thread_id: ThreadId,
        reason: impl Into<String>,
    ) {
        let reason = reason.into();
        let live_commands = {
            let state = self.state.read().await;
            state
                .commands
                .values()
                .filter(|command| command.thread_id == thread_id && command.ended_at.is_none())
                .map(|command| {
                    (
                        command.command_id.clone(),
                        command.task_id.clone(),
                        command.process_id,
                        command.runtime_owner_id.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };

        for (command_id, task_id, process_id, runtime_owner_id) in &live_commands {
            if let Some(process_id) = process_id {
                self.mark_process_status(
                    *process_id,
                    runtime_owner_id.as_deref(),
                    ManagedProcessStatus::Cleaning,
                )
                .await;
                let cleaned = self
                    .cleanup_process(*process_id, runtime_owner_id.as_deref())
                    .await;
                if cleaned {
                    self.mark_process_finished(*process_id, runtime_owner_id.as_deref())
                        .await;
                }
                self.record_event(
                    "command_process_cleanup_after_abort",
                    Some(thread_id),
                    Some(task_id.clone()),
                    Some(command_id.clone()),
                    json!({
                        "reason": &reason,
                        "process_id": process_id,
                        "runtime_owner_id": runtime_owner_id,
                        "cleaned": cleaned,
                    }),
                )
                .await;
            }
        }

        for (command_id, _, _, _) in &live_commands {
            if let Err(err) = self
                .finish_managed_command(
                    command_id.as_str(),
                    Some(-1),
                    format!("command terminated because thread aborted: {reason}").as_bytes(),
                    /*release_leases*/ true,
                )
                .await
            {
                tracing::warn!(%err, %command_id, %thread_id, "failed to finish AgentOS command after abort");
            }
        }

        let (tickets, stray_lease_ids, thread_snapshot) = {
            let mut state = self.state.write().await;
            let now = Utc::now();
            let command_ticket_ids = state
                .commands
                .values()
                .filter(|command| command.thread_id == thread_id)
                .map(|command| command.ticket_id.clone())
                .collect::<HashSet<_>>();
            let ticket_ids = state
                .tickets
                .iter()
                .filter(|(_, ticket)| {
                    ticket.thread_id == thread_id
                        && !command_ticket_ids.contains(ticket.ticket_id.as_str())
                })
                .map(|(ticket_id, _)| ticket_id.clone())
                .collect::<Vec<_>>();
            let tickets = ticket_ids
                .into_iter()
                .filter_map(|ticket_id| state.tickets.remove(ticket_id.as_str()))
                .collect::<Vec<_>>();
            let stray_lease_ids = state
                .leases
                .iter()
                .filter(|(_, lease)| lease.owner_thread_id == thread_id)
                .map(|(lease_id, _)| lease_id.clone())
                .collect::<Vec<_>>();
            let thread_snapshot = state.threads.get_mut(&thread_id).map(|thread| {
                thread.current_command_id = None;
                if matches!(
                    thread.state,
                    ThreadRuntimeState::Running
                        | ThreadRuntimeState::WaitingForLease
                        | ThreadRuntimeState::Stopping
                ) {
                    thread.state = ThreadRuntimeState::Idle;
                }
                thread.heartbeat_at = now;
                thread.clone()
            });
            (tickets, stray_lease_ids, thread_snapshot)
        };

        if !stray_lease_ids.is_empty() {
            self.release_leases(&stray_lease_ids).await;
        }
        for ticket in &tickets {
            self.persist_revoked_ticket_snapshot(ticket, reason.as_str())
                .await;
            self.record_event(
                "ticket_revoked_after_abort",
                Some(ticket.thread_id),
                Some(ticket.task_id.clone()),
                None,
                json!({
                    "ticket_id": &ticket.ticket_id,
                    "reason": &reason,
                }),
            )
            .await;
        }
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if !live_commands.is_empty() || !tickets.is_empty() || !stray_lease_ids.is_empty() {
            self.record_event(
                "thread_resources_cleaned_after_abort",
                Some(thread_id),
                None,
                None,
                json!({
                    "reason": reason,
                    "commands": live_commands
                        .iter()
                        .map(|(command_id, _, _, _)| command_id)
                        .collect::<Vec<_>>(),
                    "tickets": tickets
                        .iter()
                        .map(|ticket| &ticket.ticket_id)
                        .collect::<Vec<_>>(),
                    "stray_leases": stray_lease_ids,
                }),
            )
            .await;
        }
    }

    /// Return whether a worker has a pending structured command that should
    /// start or feed its next turn. This is intentionally non-mutating so
    /// callers can use it as a wake-up predicate without consuming commands.
    pub(crate) async fn has_claimable_runtime_command_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> bool {
        let now = Utc::now();
        let state = self.state.read().await;
        let Some(thread) = state.threads.get(&thread_id) else {
            return false;
        };
        let Some(active) = state
            .active_coordinators
            .get(thread.coordination_scope.as_str())
        else {
            return false;
        };
        state.runtime_commands.values().any(|command| {
            command.to_thread_id == thread_id
                && matches!(command.status, RuntimeCommandStatus::Pending)
                && command.expires_at > now
                && command.coordinator_epoch == active.epoch
                && command.fencing_token == active.fencing_token
        })
    }

    /// Claim runtime commands for injection into the worker's next turn.
    ///
    /// This is the runtime-lifecycle path, not a model-driven tool call: the
    /// worker does not need to remember to call `poll_runtime_commands` before
    /// receiving its assignment. Claiming a command marks it as consumed by the
    /// runtime. AssignTask commands move directly into Executing so they are
    /// not re-injected on later turns; other command types are Acked and remain
    /// visible to explicit status tools. Already-Acked commands are not claimed
    /// again, which prevents non-AssignTask commands from being injected every
    /// turn forever.
    pub(crate) async fn claim_runtime_commands_for_turn(
        &self,
        thread_id: ThreadId,
    ) -> PraxisResult<Vec<RuntimeCommandRecord>> {
        let now = Utc::now();
        let ttl = AgentOsPolicy::get().ticket_ttl();
        let (claimed, changed_commands, changed_tasks, changed_threads) = {
            let mut state = self.state.write().await;
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            let active = state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned();
            let mut claimed = Vec::new();
            let mut changed_commands = Vec::new();
            let mut changed_tasks = Vec::new();
            let mut changed_threads = Vec::new();
            let command_ids = state
                .runtime_commands
                .iter()
                .filter_map(|(command_id, command)| {
                    if command.to_thread_id != thread_id {
                        return None;
                    }
                    if !matches!(command.status, RuntimeCommandStatus::Pending) {
                        return None;
                    }
                    Some(command_id.clone())
                })
                .collect::<Vec<_>>();

            for command_id in command_ids {
                let Some(command_snapshot) = state.runtime_commands.get(&command_id).cloned()
                else {
                    continue;
                };
                let active_matches = active.as_ref().is_some_and(|active| {
                    command_snapshot.coordinator_epoch == active.epoch
                        && command_snapshot.fencing_token == active.fencing_token
                });
                let next_status = if command_snapshot.expires_at <= now {
                    RuntimeCommandStatus::Expired
                } else if !active_matches {
                    RuntimeCommandStatus::Rejected
                } else if command_snapshot.command_type == RuntimeCommandType::AssignTask {
                    RuntimeCommandStatus::Executing
                } else {
                    RuntimeCommandStatus::Acked
                };

                let Some(command) = state.runtime_commands.get_mut(&command_id) else {
                    continue;
                };
                command.status = next_status;
                command.updated_at = now;
                command.expires_at = now + ttl;
                let updated_command = command.clone();
                changed_commands.push(updated_command.clone());

                if next_status == RuntimeCommandStatus::Executing {
                    if let Some(task_id) = updated_command.task_id.as_deref() {
                        if let Some(task) = state.tasks.get_mut(task_id) {
                            task.status = TaskStatus::Running;
                            task.updated_at = now;
                            changed_tasks.push(task.clone());
                        }
                        if let Some(thread) = state.threads.get_mut(&thread_id) {
                            thread.current_task_id = Some(task_id.to_string());
                            thread.current_command_id = Some(updated_command.command_id.clone());
                            thread.state = ThreadRuntimeState::Running;
                            thread.heartbeat_at = now;
                            changed_threads.push(thread.clone());
                        }
                    }
                }

                if matches!(
                    next_status,
                    RuntimeCommandStatus::Acked | RuntimeCommandStatus::Executing
                ) {
                    claimed.push(updated_command);
                }
            }
            (claimed, changed_commands, changed_tasks, changed_threads)
        };

        for command in &changed_commands {
            self.persist_runtime_command_snapshot(command).await;
        }
        for task in &changed_tasks {
            self.persist_task_snapshot(task).await;
        }
        for thread in &changed_threads {
            self.persist_thread_snapshot(thread).await;
        }
        if !changed_commands.is_empty() {
            self.record_event(
                "runtime_commands_claimed_for_turn",
                Some(thread_id),
                None,
                None,
                json!({
                    "claimed_commands": claimed.iter().map(|command| json!({
                        "command_id": &command.command_id,
                        "command_type": command.command_type.as_str(),
                        "task_id": &command.task_id,
                        "status": format!("{:?}", command.status),
                    })).collect::<Vec<_>>(),
                    "changed_commands": changed_commands.iter().map(|command| json!({
                        "command_id": &command.command_id,
                        "status": format!("{:?}", command.status),
                    })).collect::<Vec<_>>(),
                }),
            )
            .await;
        }

        Ok(claimed)
    }

    pub(crate) async fn poll_runtime_commands(
        &self,
        thread_id: ThreadId,
        auto_ack: bool,
    ) -> PraxisResult<Vec<RuntimeCommandRecord>> {
        let now = Utc::now();
        let (commands, changed_commands) = {
            let mut state = self.state.write().await;
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            let active = state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned();
            let mut commands = Vec::new();
            let mut changed_commands = Vec::new();
            for command in state.runtime_commands.values_mut() {
                if command.to_thread_id != thread_id {
                    continue;
                }
                if !matches!(
                    command.status,
                    RuntimeCommandStatus::Pending
                        | RuntimeCommandStatus::Acked
                        | RuntimeCommandStatus::Executing
                ) {
                    continue;
                }
                if command.expires_at <= now {
                    command.status = RuntimeCommandStatus::Expired;
                    command.updated_at = now;
                    changed_commands.push(command.clone());
                    continue;
                }
                let active_matches = active.as_ref().is_some_and(|active| {
                    command.coordinator_epoch == active.epoch
                        && command.fencing_token == active.fencing_token
                });
                if !active_matches {
                    command.status = RuntimeCommandStatus::Rejected;
                    command.updated_at = now;
                    changed_commands.push(command.clone());
                    continue;
                }
                if auto_ack && command.status == RuntimeCommandStatus::Pending {
                    command.status = RuntimeCommandStatus::Acked;
                    command.updated_at = now;
                    changed_commands.push(command.clone());
                }
                commands.push(command.clone());
            }
            (commands, changed_commands)
        };

        for command in &changed_commands {
            self.persist_runtime_command_snapshot(command).await;
            self.record_event(
                "runtime_command_status_updated",
                Some(thread_id),
                command.task_id.clone(),
                None,
                json!({
                    "command_id": &command.command_id,
                    "from_thread_id": command.from_thread_id.to_string(),
                    "to_thread_id": command.to_thread_id.to_string(),
                    "command_type": command.command_type.as_str(),
                    "status": format!("{:?}", command.status),
                    "source": "poll_runtime_commands",
                }),
            )
            .await;
        }

        Ok(commands)
    }

    pub(crate) async fn issue_runtime_command(
        &self,
        from_thread_id: ThreadId,
        to_thread_id: ThreadId,
        command_type: RuntimeCommandType,
        task_id: Option<String>,
        payload: serde_json::Value,
    ) -> PraxisResult<RuntimeCommandRecord> {
        let now = Utc::now();
        let command_id = format!("runtime-command-{}", Uuid::new_v4());
        let command = {
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
            if sender.coordination_scope != receiver.coordination_scope {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime commands cannot cross coordination scopes".to_string(),
                ));
            }
            if command_type == RuntimeCommandType::AssignTask {
                let Some(task_id) = task_id.as_deref() else {
                    return Err(PraxisErr::UnsupportedOperation(
                        "AssignTask runtime commands require task_id".to_string(),
                    ));
                };
                let task = state.tasks.get(task_id).ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "AssignTask references unknown task `{task_id}`"
                    ))
                })?;
                if task.assigned_thread_id != Some(to_thread_id) {
                    return Err(PraxisErr::UnsupportedOperation(
                        "AssignTask runtime command task owner does not match receiver".to_string(),
                    ));
                }
            }
            let active = Self::claim_or_renew_active_coordinator_locked(
                &mut state,
                &sender,
                now,
                Some("issue runtime commands"),
            )?
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation(
                    "only rank-0 coordinators can issue runtime commands".to_string(),
                )
            })?;
            let coordinator_epoch = active.epoch;
            let fencing_token = active.fencing_token;
            let command = RuntimeCommandRecord {
                command_id: command_id.clone(),
                from_thread_id,
                to_thread_id,
                task_id,
                coordinator_epoch,
                fencing_token,
                command_type,
                payload,
                status: RuntimeCommandStatus::Pending,
                created_at: now,
                updated_at: now,
                expires_at: now + AgentOsPolicy::get().ticket_ttl(),
            };
            state.runtime_commands.insert(command_id, command.clone());
            command
        };

        self.persist_runtime_command_snapshot(&command).await;
        self.record_event(
            "runtime_command_issued",
            Some(command.from_thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "to_thread_id": command.to_thread_id.to_string(),
                "command_type": command.command_type.as_str(),
                "status": format!("{:?}", command.status),
                "coordinator_epoch": command.coordinator_epoch,
                "fencing_token": command.fencing_token,
            }),
        )
        .await;
        Ok(command)
    }

    pub(crate) async fn update_runtime_command_status(
        &self,
        command_id: &str,
        actor_thread_id: ThreadId,
        status: RuntimeCommandStatus,
    ) -> PraxisResult<RuntimeCommandRecord> {
        let now = Utc::now();
        let (command, thread_snapshot, task_snapshot) = {
            let mut state = self.state.write().await;
            let existing = state
                .runtime_commands
                .get(command_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown runtime command `{command_id}`"
                    ))
                })?;
            if actor_thread_id != existing.from_thread_id
                && actor_thread_id != existing.to_thread_id
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime command status can only be updated by sender or receiver".to_string(),
                ));
            }
            if matches!(
                status,
                RuntimeCommandStatus::Acked
                    | RuntimeCommandStatus::Executing
                    | RuntimeCommandStatus::Completed
            ) && actor_thread_id != existing.to_thread_id
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime command ack/execution status must be reported by the receiver"
                        .to_string(),
                ));
            }
            let active = state
                .threads
                .get(&existing.to_thread_id)
                .and_then(|thread| {
                    state
                        .active_coordinators
                        .get(thread.coordination_scope.as_str())
                })
                .cloned();
            let active_matches = active.as_ref().is_some_and(|active| {
                existing.coordinator_epoch == active.epoch
                    && existing.fencing_token == active.fencing_token
            });
            let receiver_terminal_report = actor_thread_id == existing.to_thread_id
                && matches!(
                    status,
                    RuntimeCommandStatus::Completed | RuntimeCommandStatus::Failed
                )
                && matches!(
                    existing.status,
                    RuntimeCommandStatus::Acked | RuntimeCommandStatus::Executing
                );
            let status = if receiver_terminal_report
                || matches!(
                    status,
                    RuntimeCommandStatus::Failed | RuntimeCommandStatus::Rejected
                ) {
                status
            } else if existing.expires_at <= now {
                RuntimeCommandStatus::Expired
            } else if !active_matches {
                RuntimeCommandStatus::Rejected
            } else {
                status
            };
            let command = state.runtime_commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown runtime command `{command_id}`"))
            })?;
            command.status = status;
            command.updated_at = now;
            let command_snapshot = command.clone();
            let mut thread_snapshot = None;
            let mut task_snapshot = None;
            if command_snapshot.command_type == RuntimeCommandType::AssignTask
                && let Some(task_id) = command_snapshot.task_id.as_deref()
            {
                if let Some(task) = state.tasks.get_mut(task_id) {
                    task.status = match command_snapshot.status {
                        RuntimeCommandStatus::Executing => TaskStatus::Running,
                        RuntimeCommandStatus::Completed => TaskStatus::Completed,
                        RuntimeCommandStatus::Failed | RuntimeCommandStatus::Expired => {
                            TaskStatus::Failed
                        }
                        RuntimeCommandStatus::Rejected => TaskStatus::Cancelled,
                        RuntimeCommandStatus::Pending | RuntimeCommandStatus::Acked => {
                            TaskStatus::Assigned
                        }
                    };
                    task.updated_at = now;
                    task_snapshot = Some(task.clone());
                }
                if let Some(thread) = state.threads.get_mut(&command_snapshot.to_thread_id) {
                    match command_snapshot.status {
                        RuntimeCommandStatus::Executing => {
                            thread.current_task_id = Some(task_id.to_string());
                            thread.state = ThreadRuntimeState::Running;
                        }
                        RuntimeCommandStatus::Completed
                        | RuntimeCommandStatus::Failed
                        | RuntimeCommandStatus::Rejected
                        | RuntimeCommandStatus::Expired => {
                            if thread.current_task_id.as_deref() == Some(task_id) {
                                thread.current_task_id = None;
                            }
                            thread.state = ThreadRuntimeState::Idle;
                        }
                        RuntimeCommandStatus::Pending | RuntimeCommandStatus::Acked => {
                            thread.current_task_id = Some(task_id.to_string());
                            thread.state = ThreadRuntimeState::Assigned;
                        }
                    }
                    thread.heartbeat_at = now;
                    thread_snapshot = Some(thread.clone());
                }
            }
            (command_snapshot, thread_snapshot, task_snapshot)
        };

        self.persist_runtime_command_snapshot(&command).await;
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if let Some(task) = task_snapshot {
            self.persist_task_snapshot(&task).await;
        }
        self.record_event(
            "runtime_command_status_updated",
            Some(actor_thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "from_thread_id": command.from_thread_id.to_string(),
                "to_thread_id": command.to_thread_id.to_string(),
                "command_type": command.command_type.as_str(),
                "status": format!("{:?}", command.status),
            }),
        )
        .await;
        Ok(command)
    }

    pub(super) async fn expire_runtime_commands(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            state
                .runtime_commands
                .values_mut()
                .filter(|command| {
                    matches!(
                        command.status,
                        RuntimeCommandStatus::Pending
                            | RuntimeCommandStatus::Acked
                            | RuntimeCommandStatus::Executing
                    )
                })
                .filter(|command| command.expires_at <= now)
                .map(|command| {
                    command.status = RuntimeCommandStatus::Expired;
                    command.updated_at = now;
                    command.clone()
                })
                .collect::<Vec<_>>()
        };
        for command in expired {
            self.persist_runtime_command_snapshot(&command).await;
            self.record_event(
                "runtime_command_status_updated",
                Some(command.to_thread_id),
                command.task_id.clone(),
                None,
                json!({
                    "command_id": &command.command_id,
                    "from_thread_id": command.from_thread_id.to_string(),
                    "to_thread_id": command.to_thread_id.to_string(),
                    "command_type": command.command_type.as_str(),
                    "status": format!("{:?}", command.status),
                    "source": "expire_runtime_commands",
                }),
            )
            .await;
        }
    }
}

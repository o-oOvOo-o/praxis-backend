use super::*;

pub(crate) struct AgentOsExecutionOpenRequest<'a> {
    pub(crate) thread_id: ThreadId,
    pub(crate) command: String,
    pub(crate) argv: &'a [String],
    pub(crate) cwd: &'a Path,
    pub(crate) process_id: Option<i32>,
    pub(crate) runtime_kind: Option<&'a str>,
    pub(crate) runtime_owner_id: Option<&'a str>,
}

impl AgentOs {
    pub(super) async fn command_raw_command(&self, command_id: &str) -> Option<String> {
        self.state
            .read()
            .await
            .commands
            .get(command_id)
            .map(|command| command.raw_command.clone())
    }

    pub(crate) async fn open_execution(
        self: &Arc<Self>,
        request: AgentOsExecutionOpenRequest<'_>,
    ) -> PraxisResult<ManagedCommandSpan> {
        let ticket = self
            .request_command_ticket(request.thread_id, request.argv, request.cwd)
            .await?;
        let command_id = match self
            .begin_managed_command(
                &ticket,
                request.command,
                request.argv,
                request.cwd.to_path_buf(),
                request.process_id,
                request.runtime_kind.map(str::to_string),
                request.runtime_owner_id.map(str::to_string),
            )
            .await
        {
            Ok(command_id) => command_id,
            Err(err) => {
                self.revoke_unstarted_ticket(&ticket, err.to_string()).await;
                return Err(err);
            }
        };
        Ok(ManagedCommandSpan::new(Arc::clone(self), command_id))
    }

    pub(super) async fn begin_managed_command(
        &self,
        ticket: &ExecutionTicket,
        command: String,
        argv: &[String],
        cwd: PathBuf,
        process_id: Option<i32>,
        runtime_kind: Option<String>,
        runtime_owner_id: Option<String>,
    ) -> PraxisResult<String> {
        let now = Utc::now();
        let command_id = format!("cmd-{}", Uuid::new_v4());
        let command_fingerprint = action_fingerprint(argv, &cwd, ticket.allowed_intent);
        if command_fingerprint != ticket.command_fingerprint {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket command fingerprint does not match requested command".to_string(),
            ));
        }
        if normalize_path_for_scope(&cwd) != normalize_path_for_scope(&ticket.cwd) {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket cwd does not match requested command".to_string(),
            ));
        }
        let baseline_dirty_files = if requires_dirty_audit(ticket.allowed_intent) {
            audit_git_dirty_files(cwd.as_path()).await
        } else {
            Vec::new()
        };
        let baseline_dirty_fingerprints =
            dirty_file_fingerprints(cwd.as_path(), &baseline_dirty_files);
        let record = CommandRecord {
            command_id: command_id.clone(),
            ticket_id: ticket.ticket_id.clone(),
            task_id: ticket.task_id.clone(),
            thread_id: ticket.thread_id,
            intent: ticket.allowed_intent,
            intent_plan_id: ticket.intent_plan_id.clone(),
            command_fingerprint,
            raw_command: command,
            cwd,
            process_id,
            runtime_kind: runtime_kind.clone(),
            runtime_owner_id: runtime_owner_id.clone(),
            started_at: now,
            ended_at: None,
            exit_code: None,
            lease_ids: ticket.lease_ids.clone(),
            artifacts: Vec::new(),
            baseline_dirty_files,
            baseline_dirty_fingerprints,
            dirty_files: Vec::new(),
        };

        let lease_snapshots = {
            let mut state = self.state.write().await;
            self.validate_ticket_locked(&state, ticket)?;
            if let Some(thread) = state.threads.get_mut(&ticket.thread_id) {
                thread.current_command_id = Some(command_id.clone());
                thread.state = ThreadRuntimeState::Running;
                thread.heartbeat_at = now;
            }
            if let Some(task) = state.tasks.get_mut(&ticket.task_id) {
                task.status = TaskStatus::Running;
                task.updated_at = now;
            }
            let mut lease_snapshots = Vec::new();
            for lease_id in &ticket.lease_ids {
                if let Some(lease) = state.leases.get_mut(lease_id) {
                    lease.command_id = Some(command_id.clone());
                    lease.process_id = process_id;
                    lease.runtime_owner_id = runtime_owner_id.clone();
                    lease_snapshots.push(lease.clone());
                }
            }
            if let Some(process_id) = process_id {
                let runtime_kind = runtime_kind
                    .clone()
                    .unwrap_or_else(|| runtime_kind_for_intent(ticket.allowed_intent).to_string());
                let process = ManagedProcessRecord {
                    process_id,
                    command_id: command_id.clone(),
                    task_id: ticket.task_id.clone(),
                    thread_id: ticket.thread_id,
                    cwd: record.cwd.clone(),
                    runtime_kind,
                    runtime_owner_id: runtime_owner_id.clone(),
                    started_at: now,
                    last_heartbeat: now,
                    ended_at: None,
                    status: ManagedProcessStatus::Running,
                };
                let process_key =
                    process_registry_key(process_id, process.runtime_owner_id.as_deref());
                state.processes.insert(process_key, process);
            }
            state.commands.insert(command_id.clone(), record.clone());
            lease_snapshots
        };

        for lease in lease_snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
        self.persist_started_ticket_snapshot(ticket, command_id.as_str())
            .await;
        self.persist_command_snapshot(&record).await;
        if let Some(process_id) = process_id
            && let Some(process) = self
                .process_snapshot(process_id, runtime_owner_id.as_deref())
                .await
        {
            self.persist_process_snapshot(&process).await;
        }
        self.record_event(
            "command_started",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            Some(command_id.clone()),
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
        Ok(command_id)
    }

    pub(super) async fn attach_process_to_managed_command(
        &self,
        command_id: &str,
        process_id: i32,
    ) -> PraxisResult<()> {
        let now = Utc::now();
        let (command_snapshot, process_snapshot, lease_snapshots) = {
            let mut state = self.state.write().await;
            let command = state.commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            if let Some(existing_process_id) = command.process_id {
                if existing_process_id == process_id {
                    return Ok(());
                }
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "command `{command_id}` already has process id `{existing_process_id}`"
                )));
            }

            command.process_id = Some(process_id);
            let runtime_kind = command
                .runtime_kind
                .clone()
                .unwrap_or_else(|| runtime_kind_for_intent(command.intent).to_string());
            let runtime_owner_id = command.runtime_owner_id.clone();
            let command_snapshot = command.clone();

            let mut lease_snapshots = Vec::new();
            for lease_id in &command_snapshot.lease_ids {
                if let Some(lease) = state.leases.get_mut(lease_id) {
                    lease.process_id = Some(process_id);
                    lease.runtime_owner_id = runtime_owner_id.clone();
                    lease_snapshots.push(lease.clone());
                }
            }

            let process = ManagedProcessRecord {
                process_id,
                command_id: command_id.to_string(),
                task_id: command_snapshot.task_id.clone(),
                thread_id: command_snapshot.thread_id,
                cwd: command_snapshot.cwd.clone(),
                runtime_kind,
                runtime_owner_id,
                started_at: now,
                last_heartbeat: now,
                ended_at: None,
                status: ManagedProcessStatus::Running,
            };
            let process_key = process_registry_key(process_id, process.runtime_owner_id.as_deref());
            state.processes.insert(process_key, process.clone());
            (command_snapshot, process, lease_snapshots)
        };

        for lease in lease_snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
        self.persist_command_snapshot(&command_snapshot).await;
        self.persist_process_snapshot(&process_snapshot).await;
        self.record_event(
            "command_process_attached",
            Some(command_snapshot.thread_id),
            Some(command_snapshot.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "process_id": process_id,
                "runtime_kind": process_snapshot.runtime_kind,
                "runtime_owner_id": process_snapshot.runtime_owner_id,
            }),
        )
        .await;
        Ok(())
    }

    pub(super) async fn finish_managed_command(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        raw_output: &[u8],
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        self.finish_managed_command_with_output_source(
            command_id,
            exit_code,
            ManagedCommandOutputSource::Bytes(raw_output),
            release_leases,
        )
        .await
    }

    pub(super) async fn finish_managed_command_with_spooled_output(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        output_spool: ExecOutputSpool,
        fallback_raw_output: &[u8],
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        self.finish_managed_command_with_output_source(
            command_id,
            exit_code,
            ManagedCommandOutputSource::Spool {
                spool: output_spool,
                fallback_raw_output,
            },
            release_leases,
        )
        .await
    }

    pub(super) async fn finish_managed_command_with_output_source(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        output_source: ManagedCommandOutputSource<'_>,
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        let now = Utc::now();
        let (mut command, mut thread_snapshot, mut task_snapshot, lease_ids) = {
            let mut state = self.state.write().await;
            let (command_snapshot, process_ref) = {
                let command = state.commands.get_mut(command_id).ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
                })?;
                command.ended_at = Some(now);
                command.exit_code = exit_code;
                let process_ref = command
                    .process_id
                    .map(|process_id| (process_id, command.runtime_owner_id.clone()));
                (command.clone(), process_ref)
            };
            if let Some((process_id, runtime_owner_id)) = process_ref {
                let process_key = process_registry_key(process_id, runtime_owner_id.as_deref());
                if let Some(process) = state.processes.get_mut(process_key.as_str()) {
                    process.last_heartbeat = now;
                    process.ended_at = Some(now);
                    process.status = ManagedProcessStatus::Finished;
                }
            }
            let has_active_runtime_command = has_active_assign_runtime_command_locked(
                &state,
                command_snapshot.thread_id,
                command_snapshot.task_id.as_str(),
            );
            let lease_ids = command_snapshot.lease_ids.clone();
            let thread_snapshot =
                if let Some(thread) = state.threads.get_mut(&command_snapshot.thread_id) {
                    if thread.current_command_id.as_deref() == Some(command_id) {
                        thread.current_command_id = None;
                    }
                    if has_active_runtime_command {
                        thread.current_task_id = Some(command_snapshot.task_id.clone());
                        if !matches!(
                            thread.state,
                            ThreadRuntimeState::WaitingForLease
                                | ThreadRuntimeState::WaitingForCoordinator
                                | ThreadRuntimeState::Stopping
                                | ThreadRuntimeState::Stopped
                                | ThreadRuntimeState::Failed
                                | ThreadRuntimeState::Completed
                        ) {
                            thread.state = ThreadRuntimeState::Running;
                        }
                    } else {
                        thread.state = ThreadRuntimeState::Idle;
                    }
                    thread.heartbeat_at = now;
                    Some(thread.clone())
                } else {
                    None
                };
            let task_snapshot = if let Some(task) = state.tasks.get_mut(&command_snapshot.task_id) {
                if !matches!(
                    task.status,
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
                ) {
                    task.status = if has_active_runtime_command {
                        TaskStatus::Running
                    } else {
                        TaskStatus::Assigned
                    };
                }
                task.updated_at = now;
                Some(task.clone())
            } else {
                None
            };
            (command_snapshot, thread_snapshot, task_snapshot, lease_ids)
        };

        let artifact_id = if output_source.is_empty() {
            None
        } else {
            let artifact_result = self
                .create_command_output_artifact(&command, command_id, exit_code, &output_source)
                .await;
            if let ManagedCommandOutputSource::Spool { spool, .. } = &output_source {
                spool.cleanup().await;
            }
            Some(artifact_result?)
        };

        if let Some(artifact_id) = artifact_id.clone() {
            command.artifacts.push(artifact_id.clone());
            let mut state = self.state.write().await;
            state
                .commands
                .insert(command_id.to_string(), command.clone());
        }

        if let Some(outcome) = self.audit_finished_command_dirty_files(&command).await? {
            let dirty_file_report =
                format_dirty_file_report(&outcome.dirty_files, outcome.violation_path.as_ref());
            let dirty_file_artifact_id = self
                .create_blob_artifact(
                    outcome.command.task_id.clone(),
                    outcome.command.thread_id,
                    ArtifactType::DirtyFileReport,
                    "dirty-file-report",
                    dirty_file_report.clone(),
                    json!({
                        "command_id": command_id,
                        "dirty_files": outcome
                            .dirty_files
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>(),
                        "violation_path": outcome
                            .violation_path
                            .as_ref()
                            .map(|path| path.display().to_string()),
                    }),
                    "txt",
                    dirty_file_report.as_bytes(),
                )
                .await?;
            command = outcome.command;
            command.artifacts.push(dirty_file_artifact_id.clone());
            {
                let mut state = self.state.write().await;
                state
                    .commands
                    .insert(command_id.to_string(), command.clone());
            }
            thread_snapshot = outcome.thread_snapshot.or(thread_snapshot);
            task_snapshot = outcome.task_snapshot.or(task_snapshot);
            self.record_event(
                "command_dirty_file_audit",
                Some(command.thread_id),
                Some(command.task_id.clone()),
                Some(command.command_id.clone()),
                json!({
                    "artifact_id": dirty_file_artifact_id,
                    "dirty_files": command
                        .dirty_files
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>(),
                    "violation_path": outcome
                        .violation_path
                        .as_ref()
                        .map(|path| path.display().to_string()),
                }),
            )
            .await;
        }

        if release_leases {
            self.release_leases(&lease_ids).await;
        }
        let finished_ticket = {
            let mut state = self.state.write().await;
            state.tickets.remove(command.ticket_id.as_str())
        };
        if let Some(ticket) = finished_ticket.as_ref() {
            self.persist_finished_ticket_snapshot(ticket, None).await;
        }
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if let Some(task) = task_snapshot {
            self.persist_task_snapshot(&task).await;
        }
        self.persist_command_snapshot(&command).await;
        if let Some(process_id) = command.process_id
            && let Some(process) = self
                .process_snapshot(process_id, command.runtime_owner_id.as_deref())
                .await
        {
            self.persist_process_snapshot(&process).await;
        }
        self.record_event(
            "command_finished",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "exit_code": exit_code,
                "artifact_id": artifact_id,
                "leases_released": release_leases,
                "runtime_kind": command.runtime_kind.as_deref(),
                "runtime_owner_id": command.runtime_owner_id.as_deref(),
                "process_id": command.process_id,
            }),
        )
        .await;
        Ok(artifact_id)
    }

    pub(super) async fn checkpoint_managed_command(
        &self,
        command_id: &str,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        if raw_output.is_empty() {
            return Ok(None);
        }
        let command = self
            .state
            .read()
            .await
            .commands
            .get(command_id)
            .cloned()
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
        self.renew_command_leases(&command).await;
        let artifact_id = self
            .create_blob_artifact(
                command.task_id.clone(),
                command.thread_id,
                artifact_type_for_intent(command.intent),
                "command-checkpoint",
                summarize_output(raw_output),
                json!({
                    "command_id": command_id,
                    "bytes": raw_output.len(),
                    "checkpoint": true,
                }),
                "log",
                raw_output,
            )
            .await?;
        let command_snapshot = {
            let mut state = self.state.write().await;
            let process_ref = state.commands.get(command_id).and_then(|command| {
                command
                    .process_id
                    .map(|process_id| (process_id, command.runtime_owner_id.clone()))
            });
            if let Some((process_id, runtime_owner_id)) = process_ref {
                let process_key = process_registry_key(process_id, runtime_owner_id.as_deref());
                if let Some(process) = state.processes.get_mut(process_key.as_str()) {
                    process.last_heartbeat = Utc::now();
                }
            }
            if let Some(command) = state.commands.get_mut(command_id) {
                command.artifacts.push(artifact_id.clone());
                Some(command.clone())
            } else {
                None
            }
        };
        if let Some(command) = command_snapshot {
            self.persist_command_snapshot(&command).await;
            if let Some(process_id) = command.process_id
                && let Some(process) = self
                    .process_snapshot(process_id, command.runtime_owner_id.as_deref())
                    .await
            {
                self.persist_process_snapshot(&process).await;
            }
        }
        self.record_event(
            "command_checkpoint",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "artifact_id": artifact_id,
            }),
        )
        .await;
        Ok(Some(artifact_id))
    }

    pub(crate) async fn checkpoint_managed_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self
            .command_id_for_process(process_id, runtime_owner_id)
            .await
        else {
            return Ok(None);
        };
        self.checkpoint_managed_command(command_id.as_str(), raw_output)
            .await
    }

    pub(crate) async fn finish_managed_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        exit_code: Option<i32>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self
            .command_id_for_process(process_id, runtime_owner_id)
            .await
        else {
            return Ok(None);
        };
        self.finish_managed_command(command_id.as_str(), exit_code, raw_output, true)
            .await
    }

    pub(super) async fn audit_finished_command_dirty_files(
        &self,
        command: &CommandRecord,
    ) -> PraxisResult<Option<DirtyAuditOutcome>> {
        if !requires_dirty_audit(command.intent) {
            return Ok(None);
        }

        let after_dirty_files = audit_git_dirty_files(command.cwd.as_path()).await;
        let dirty_files = dirty_file_delta(
            command.cwd.as_path(),
            &command.baseline_dirty_files,
            &command.baseline_dirty_fingerprints,
            &after_dirty_files,
        );
        if dirty_files.is_empty() {
            return Ok(None);
        }

        let (command_snapshot, thread_snapshot, task_snapshot, violation) = {
            let mut state = self.state.write().await;
            let task_snapshot = state.tasks.get(&command.task_id).cloned();
            let profile = state
                .threads
                .get(&command.thread_id)
                .and_then(|thread| state.profiles.get(thread.profile_id.as_str()))
                .cloned();
            let violation_path = task_snapshot.as_ref().and_then(|task| {
                dirty_files
                    .iter()
                    .find(|path| {
                        !dirty_file_allowed_by_task(task, path)
                            || !profile
                                .as_ref()
                                .is_some_and(|profile| profile.path_scopes.allows(path))
                    })
                    .cloned()
            });

            let command_record = state.commands.get_mut(&command.command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{}`", command.command_id))
            })?;
            push_unique_dirty_files(&mut command_record.dirty_files, &dirty_files);
            let command_snapshot = command_record.clone();

            let (thread_snapshot, task_snapshot) = if violation_path.is_some() {
                let thread_snapshot = state.threads.get_mut(&command.thread_id).map(|thread| {
                    thread.state = ThreadRuntimeState::Failed;
                    thread.heartbeat_at = Utc::now();
                    thread.clone()
                });
                let task_snapshot = state.tasks.get_mut(&command.task_id).map(|task| {
                    task.status = TaskStatus::Failed;
                    task.updated_at = Utc::now();
                    task.clone()
                });
                (thread_snapshot, task_snapshot)
            } else {
                (None, task_snapshot)
            };

            (
                command_snapshot,
                thread_snapshot,
                task_snapshot,
                violation_path,
            )
        };

        self.persist_command_snapshot(&command_snapshot).await;
        if let Some(thread) = thread_snapshot.as_ref() {
            self.persist_thread_snapshot(thread).await;
        }
        if let Some(task) = task_snapshot.as_ref() {
            self.persist_task_snapshot(task).await;
        }

        if let Some(path) = violation.as_ref() {
            self.record_event(
                "policy_violation",
                Some(command_snapshot.thread_id),
                Some(command_snapshot.task_id.clone()),
                Some(command_snapshot.command_id.clone()),
                json!({
                    "reason": "dirty_file_outside_task_or_profile_scope",
                    "path": path.display().to_string(),
                    "detected_by": "post_command_dirty_audit",
                }),
            )
            .await;
        }

        Ok(Some(DirtyAuditOutcome {
            command: command_snapshot,
            thread_snapshot,
            task_snapshot,
            dirty_files,
            violation_path: violation,
        }))
    }

    pub(super) async fn record_command_dirty_files(
        &self,
        command_id: &str,
        dirty_files: Vec<PathBuf>,
    ) -> PraxisResult<()> {
        if dirty_files.is_empty() {
            return Ok(());
        }
        let violation = {
            let state = self.state.read().await;
            let command = state.commands.get(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            let task = state.tasks.get(&command.task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "command `{command_id}` references unknown task `{}`",
                    command.task_id
                ))
            })?;
            let profile = state
                .threads
                .get(&command.thread_id)
                .and_then(|thread| state.profiles.get(thread.profile_id.as_str()));
            dirty_files
                .iter()
                .find(|path| {
                    !dirty_file_allowed_by_task(task, path)
                        || !profile.is_some_and(|profile| profile.path_scopes.allows(path))
                })
                .map(|path| (command.clone(), task.clone(), path.clone()))
        };
        if let Some((command, task, path)) = violation {
            self.record_event(
                "policy_violation",
                Some(command.thread_id),
                Some(command.task_id.clone()),
                Some(command.command_id.clone()),
                json!({
                    "reason": "dirty_file_outside_task_or_profile_scope",
                    "path": path.display().to_string(),
                    "task_scope": task.scope,
                }),
            )
            .await;
            return Err(PraxisErr::UnsupportedOperation(format!(
                "dirty file `{}` is outside AgentOS task/profile scope",
                path.display()
            )));
        }
        let command = {
            let mut state = self.state.write().await;
            let command = state.commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            push_unique_dirty_files(&mut command.dirty_files, &dirty_files);
            command.clone()
        };
        self.persist_command_snapshot(&command).await;
        self.record_event(
            "command_dirty_files_recorded",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command.command_id.clone()),
            json!({
                "dirty_files": command
                    .dirty_files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
            }),
        )
        .await;
        Ok(())
    }

    pub(super) async fn command_id_for_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> Option<String> {
        let state = self.state.read().await;
        let process_key = process_registry_key(process_id, runtime_owner_id);
        if let Some(process) = state.processes.get(process_key.as_str())
            && process.status != ManagedProcessStatus::Finished
        {
            return Some(process.command_id.clone());
        }
        state
            .commands
            .values()
            .find(|command| {
                command.process_id == Some(process_id)
                    && command.runtime_owner_id.as_deref() == runtime_owner_id
                    && command.ended_at.is_none()
            })
            .map(|command| command.command_id.clone())
    }

    pub(super) async fn process_snapshot(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> Option<ManagedProcessRecord> {
        let process_key = process_registry_key(process_id, runtime_owner_id);
        self.state
            .read()
            .await
            .processes
            .get(process_key.as_str())
            .cloned()
    }

    pub(super) async fn mark_process_status(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        status: ManagedProcessStatus,
    ) {
        let process_key = process_registry_key(process_id, runtime_owner_id);
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(process) = state.processes.get_mut(process_key.as_str()) else {
                return;
            };
            process.status = status;
            process.last_heartbeat = Utc::now();
            process.clone()
        };
        self.persist_process_snapshot(&snapshot).await;
    }

    pub(super) async fn mark_process_finished(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) {
        let now = Utc::now();
        let process_key = process_registry_key(process_id, runtime_owner_id);
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(process) = state.processes.get_mut(process_key.as_str()) else {
                return;
            };
            process.status = ManagedProcessStatus::Finished;
            process.last_heartbeat = now;
            process.ended_at.get_or_insert(now);
            process.clone()
        };
        self.persist_process_snapshot(&snapshot).await;
    }

    pub(super) async fn renew_command_leases(&self, command: &CommandRecord) {
        let snapshots = {
            let mut state = self.state.write().await;
            command
                .lease_ids
                .iter()
                .filter_map(|lease_id| {
                    let lease = state.leases.get_mut(lease_id)?;
                    lease.expires_at = Some(Utc::now() + AgentOsPolicy::get().lease_ttl());
                    Some(lease.clone())
                })
                .collect::<Vec<_>>()
        };
        for lease in snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
    }
}

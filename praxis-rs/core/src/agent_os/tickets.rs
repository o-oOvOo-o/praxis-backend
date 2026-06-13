use super::*;

impl AgentOsRuntime {
    pub(crate) async fn request_command_ticket(
        &self,
        thread_id: ThreadId,
        command: &[String],
        cwd: &Path,
    ) -> PraxisResult<ExecutionTicket> {
        self.expire_leases().await;
        self.expire_intent_plans().await;
        let intent = classify_command(command, cwd);
        let now = Utc::now();
        let command_fingerprint = action_fingerprint(command, cwd, intent.kind);
        let ticket_id = format!("exec-ticket-{}", Uuid::new_v4());
        let (thread, task, profile, coordinator_epoch, coordinator_fencing, intent_plan_id) = {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "AgentOS thread `{thread_id}` is not registered"
                ))
            })?;
            let task_id = thread.current_task_id.clone().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(
                    "side-effectful action rejected: thread has no current task_id".to_string(),
                )
            })?;
            let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "current task `{task_id}` is not registered"
                ))
            })?;
            let profile = state
                .profiles
                .get(&thread.profile_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown capability profile `{}`",
                        thread.profile_id
                    ))
                })?;
            let active = if thread.rank == COORDINATOR_RANK {
                Self::claim_or_renew_active_coordinator_locked(
                    &mut state,
                    &thread,
                    now,
                    Some("run side-effectful actions"),
                )?
            } else {
                state
                    .active_coordinators
                    .get(thread.coordination_scope.as_str())
                    .cloned()
            };
            let intent_plan_id = Self::find_matching_intent_plan_locked(
                &state,
                thread_id,
                task.task_id.as_str(),
                intent.kind,
                command_fingerprint.as_str(),
                cwd,
            )
            .map(|plan| plan.plan_id.clone());
            (
                thread,
                task,
                profile,
                active.as_ref().map(|value| value.epoch).unwrap_or(0),
                active
                    .as_ref()
                    .map(|value| value.fencing_token)
                    .unwrap_or(0),
                intent_plan_id,
            )
        };

        profile
            .validate_command_intent(&intent, command, cwd)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let intent_plan_id = intent_plan_id.ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "execution ticket rejected: command has no matching AgentOS intent preflight plan"
                    .to_string(),
            )
        })?;
        let ticket_intent_plan_id = intent_plan_id.clone();

        let lease_ids = match self
            .acquire_required_leases(
                thread_id,
                task.task_id.as_str(),
                thread.priority.max(task.priority),
                &intent.required_resources,
            )
            .await
        {
            Ok(lease_ids) => lease_ids,
            Err(err) => {
                self.mark_thread_state(thread_id, ThreadRuntimeState::WaitingForLease)
                    .await;
                return Err(err);
            }
        };

        let ticket = ExecutionTicket {
            ticket_id,
            task_id: task.task_id,
            thread_id,
            coordination_scope: thread.coordination_scope,
            allowed_intent: intent.kind,
            intent_plan_id: Some(intent_plan_id),
            command_fingerprint,
            cwd: cwd.to_path_buf(),
            risk_level: intent.risk_level.clone(),
            capabilities: required_capabilities,
            lease_ids,
            file_scopes: profile.path_scopes.allow.clone(),
            token_budget: task.token_budget,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
            fencing_token: coordinator_fencing,
            coordinator_epoch,
            created_at: now,
        };

        let plan_snapshot_result = {
            let mut state = self.state.write().await;
            match state.intent_plans.get_mut(ticket_intent_plan_id.as_str()) {
                Some(plan)
                    if plan.status != CommandIntentPlanStatus::Pending
                        || plan.expires_at <= now =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "execution ticket rejected: intent plan `{ticket_intent_plan_id}` is not pending"
                    )))
                }
                Some(plan)
                    if plan.thread_id != ticket.thread_id
                        || plan.task_id != ticket.task_id
                        || plan.intent != ticket.allowed_intent
                        || plan.command_fingerprint != ticket.command_fingerprint
                        || normalize_path_for_scope(plan.cwd.as_path())
                            != normalize_path_for_scope(ticket.cwd.as_path()) =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "execution ticket rejected: intent plan `{ticket_intent_plan_id}` does not match ticket action"
                    )))
                }
                Some(plan) => {
                    plan.status = CommandIntentPlanStatus::Consumed;
                    plan.consumed_by_ticket_id = Some(ticket.ticket_id.clone());
                    let plan_snapshot = plan.clone();
                    state
                        .tickets
                        .insert(ticket.ticket_id.clone(), ticket.clone());
                    Ok(plan_snapshot)
                }
                None => Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing intent plan `{ticket_intent_plan_id}`"
                ))),
            }
        };
        let plan_snapshot_result = match plan_snapshot_result {
            Ok(plan) => plan,
            Err(err) => {
                self.release_leases(&ticket.lease_ids).await;
                return Err(err);
            }
        };
        self.persist_ticket_snapshot(&ticket).await;
        self.persist_intent_plan_snapshot(&plan_snapshot_result)
            .await;
        self.record_event(
            "ticket_issued",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "intent": ticket.allowed_intent.as_str(),
                "leases": &ticket.lease_ids,
            }),
        )
        .await;
        self.record_event(
            "command_intent_plan_consumed",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "plan_id": plan_snapshot_result.plan_id,
                "ticket_id": &ticket.ticket_id,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
        Ok(ticket)
    }

    pub(crate) async fn request_mutating_tool_ticket(
        &self,
        thread_id: ThreadId,
        tool_name: &str,
        arguments_fingerprint_source: &str,
    ) -> PraxisResult<ExecutionTicket> {
        self.expire_leases().await;
        self.expire_intent_plans().await;
        let intent = classify_mutating_tool(tool_name);
        let now = Utc::now();
        let action = vec![
            format!("tool:{tool_name}"),
            arguments_fingerprint_source.to_string(),
        ];
        let (
            thread,
            task,
            profile,
            coordinator_epoch,
            coordinator_fencing,
            intent_plan_id,
            command_fingerprint,
        ) = {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "AgentOS thread `{thread_id}` is not registered"
                ))
            })?;
            let task_id = thread.current_task_id.clone().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(
                    "side-effectful tool rejected: thread has no current task_id".to_string(),
                )
            })?;
            let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "current task `{task_id}` is not registered"
                ))
            })?;
            let profile = state
                .profiles
                .get(&thread.profile_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown capability profile `{}`",
                        thread.profile_id
                    ))
                })?;
            let active = if thread.rank == COORDINATOR_RANK {
                Self::claim_or_renew_active_coordinator_locked(
                    &mut state,
                    &thread,
                    now,
                    Some("run side-effectful tools"),
                )?
            } else {
                state
                    .active_coordinators
                    .get(thread.coordination_scope.as_str())
                    .cloned()
            };
            let command_fingerprint =
                action_fingerprint(&action, thread.cwd.as_path(), intent.kind);
            let intent_plan_id = Self::find_matching_intent_plan_locked(
                &state,
                thread_id,
                task.task_id.as_str(),
                intent.kind,
                command_fingerprint.as_str(),
                thread.cwd.as_path(),
            )
            .map(|plan| plan.plan_id.clone());
            (
                thread,
                task,
                profile,
                active.as_ref().map(|value| value.epoch).unwrap_or(0),
                active
                    .as_ref()
                    .map(|value| value.fencing_token)
                    .unwrap_or(0),
                intent_plan_id,
                command_fingerprint,
            )
        };

        profile
            .validate_tool_intent(&intent)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let intent_plan_id = intent_plan_id.ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "tool ticket rejected: tool has no matching AgentOS intent preflight plan"
                    .to_string(),
            )
        })?;
        let ticket_intent_plan_id = intent_plan_id.clone();

        let lease_ids = match self
            .acquire_required_leases(
                thread_id,
                task.task_id.as_str(),
                thread.priority.max(task.priority),
                &intent.required_resources,
            )
            .await
        {
            Ok(lease_ids) => lease_ids,
            Err(err) => {
                self.mark_thread_state(thread_id, ThreadRuntimeState::WaitingForLease)
                    .await;
                return Err(err);
            }
        };

        let ticket = ExecutionTicket {
            ticket_id: format!("exec-ticket-{}", Uuid::new_v4()),
            task_id: task.task_id,
            thread_id,
            coordination_scope: thread.coordination_scope,
            allowed_intent: intent.kind,
            intent_plan_id: Some(intent_plan_id),
            command_fingerprint,
            cwd: thread.cwd,
            risk_level: intent.risk_level.clone(),
            capabilities: required_capabilities,
            lease_ids,
            file_scopes: profile.path_scopes.allow.clone(),
            token_budget: task.token_budget,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
            fencing_token: coordinator_fencing,
            coordinator_epoch,
            created_at: now,
        };

        let plan_snapshot_result = {
            let mut state = self.state.write().await;
            match state.intent_plans.get_mut(ticket_intent_plan_id.as_str()) {
                Some(plan)
                    if plan.status != CommandIntentPlanStatus::Pending
                        || plan.expires_at <= now =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "tool ticket rejected: intent plan `{ticket_intent_plan_id}` is not pending"
                    )))
                }
                Some(plan)
                    if plan.thread_id != ticket.thread_id
                        || plan.task_id != ticket.task_id
                        || plan.intent != ticket.allowed_intent
                        || plan.command_fingerprint != ticket.command_fingerprint
                        || normalize_path_for_scope(plan.cwd.as_path())
                            != normalize_path_for_scope(ticket.cwd.as_path()) =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "tool ticket rejected: intent plan `{ticket_intent_plan_id}` does not match ticket action"
                    )))
                }
                Some(plan) => {
                    plan.status = CommandIntentPlanStatus::Consumed;
                    plan.consumed_by_ticket_id = Some(ticket.ticket_id.clone());
                    let plan_snapshot = plan.clone();
                    state
                        .tickets
                        .insert(ticket.ticket_id.clone(), ticket.clone());
                    Ok(plan_snapshot)
                }
                None => Err(PraxisErr::UnsupportedOperation(format!(
                    "tool ticket references missing intent plan `{ticket_intent_plan_id}`"
                ))),
            }
        };
        let plan_snapshot_result = match plan_snapshot_result {
            Ok(plan) => plan,
            Err(err) => {
                self.release_leases(&ticket.lease_ids).await;
                return Err(err);
            }
        };
        self.persist_ticket_snapshot(&ticket).await;
        self.persist_intent_plan_snapshot(&plan_snapshot_result)
            .await;
        self.record_event(
            "tool_ticket_issued",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "tool": tool_name,
                "intent": ticket.allowed_intent.as_str(),
                "leases": &ticket.lease_ids,
            }),
        )
        .await;
        self.record_event(
            "command_intent_plan_consumed",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "plan_id": plan_snapshot_result.plan_id,
                "ticket_id": &ticket.ticket_id,
                "tool": tool_name,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
        Ok(ticket)
    }

    pub(crate) async fn finish_tool_ticket(
        &self,
        ticket: &ExecutionTicket,
        success: bool,
    ) -> PraxisResult<()> {
        let removed_ticket = {
            let mut state = self.state.write().await;
            state.tickets.remove(ticket.ticket_id.as_str())
        };
        if removed_ticket.is_none() {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "tool ticket `{}` is not live",
                ticket.ticket_id
            )));
        }
        let lease_ids = ticket.lease_ids.clone();
        self.release_leases(&lease_ids).await;
        self.persist_finished_ticket_snapshot(ticket, Some(success))
            .await;
        self.record_event(
            "tool_ticket_finished",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": ticket.ticket_id,
                "success": success,
            }),
        )
        .await;
        Ok(())
    }

    pub(super) async fn revoke_unstarted_ticket(&self, ticket: &ExecutionTicket, reason: String) {
        {
            let mut state = self.state.write().await;
            state.tickets.remove(ticket.ticket_id.as_str());
        }
        self.release_leases(&ticket.lease_ids).await;
        self.persist_revoked_ticket_snapshot(ticket, reason.as_str())
            .await;
        self.record_event(
            "ticket_revoked",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "reason": reason,
                "stage": "begin_managed_command",
            }),
        )
        .await;
    }

    pub(super) fn validate_ticket_locked(
        &self,
        state: &AgentOsState,
        ticket: &ExecutionTicket,
    ) -> PraxisResult<()> {
        if ticket.expires_at <= Utc::now() {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "execution ticket `{}` has expired",
                ticket.ticket_id
            )));
        }
        if let Some(active) = state
            .active_coordinators
            .get(ticket.coordination_scope.as_str())
            && (ticket.coordinator_epoch != active.epoch
                || ticket.fencing_token != active.fencing_token)
        {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket coordinator epoch is stale".to_string(),
            ));
        }
        for lease_id in &ticket.lease_ids {
            let lease = state.leases.get(lease_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing lease `{lease_id}`"
                ))
            })?;
            if lease.owner_thread_id != ticket.thread_id || lease.task_id != ticket.task_id {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references lease `{lease_id}` owned by another task or thread"
                )));
            }
        }
        if let Some(plan_id) = ticket.intent_plan_id.as_deref() {
            let plan = state.intent_plans.get(plan_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing intent plan `{plan_id}`"
                ))
            })?;
            if plan.status != CommandIntentPlanStatus::Consumed
                || plan.consumed_by_ticket_id.as_deref() != Some(ticket.ticket_id.as_str())
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket intent plan `{plan_id}` was not consumed by this ticket"
                )));
            }
            if plan.thread_id != ticket.thread_id
                || plan.task_id != ticket.task_id
                || plan.intent != ticket.allowed_intent
                || plan.command_fingerprint != ticket.command_fingerprint
                || normalize_path_for_scope(plan.cwd.as_path())
                    != normalize_path_for_scope(ticket.cwd.as_path())
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket intent plan `{plan_id}` does not match ticket action"
                )));
            }
        }
        Ok(())
    }
}

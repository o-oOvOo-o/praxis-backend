use super::*;

impl AgentOsRuntime {
    pub(crate) async fn preflight_command_intent(
        &self,
        thread_id: ThreadId,
        command: &[String],
        cwd: &Path,
    ) -> PraxisResult<CommandIntentPlan> {
        self.note_runtime_command_activity(thread_id, RuntimeCommandActivity::WorkerStartedCommand)
            .await;
        let intent = classify_command(command, cwd);
        let now = Utc::now();
        let (thread, task, profile) = {
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
            (thread, task, profile)
        };

        profile
            .validate_command_intent(&intent, command, cwd)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let plan = CommandIntentPlan {
            plan_id: format!("intent-plan-{}", Uuid::new_v4()),
            task_id: task.task_id,
            thread_id,
            intent: intent.kind,
            confidence: intent.confidence,
            command_fingerprint: action_fingerprint(command, cwd, intent.kind),
            command: command.to_vec(),
            cwd: cwd.to_path_buf(),
            required_capabilities,
            required_resources: intent.required_resources,
            side_effects: intent.side_effects,
            risk_level: intent.risk_level,
            status: CommandIntentPlanStatus::Pending,
            consumed_by_ticket_id: None,
            created_at: now,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
        };

        self.insert_intent_plan(&plan).await;
        self.persist_intent_plan_snapshot(&plan).await;
        self.record_event(
            "command_intent_preflight",
            Some(thread.thread_id),
            Some(plan.task_id.clone()),
            None,
            json!({
                "plan_id": &plan.plan_id,
                "intent": plan.intent.as_str(),
                "confidence": plan.confidence,
                "risk_level": &plan.risk_level,
                "status": format!("{:?}", plan.status),
                "expires_at": plan.expires_at.to_rfc3339(),
                "required_capabilities": &plan.required_capabilities,
                "required_resources": plan
                    .required_resources
                    .iter()
                    .map(ResourceRequirement::key)
                    .collect::<Vec<_>>(),
                "cwd": &plan.cwd,
            }),
        )
        .await;
        Ok(plan)
    }

    pub(crate) async fn preflight_mutating_tool_intent(
        &self,
        thread_id: ThreadId,
        tool_name: &str,
        arguments_fingerprint_source: &str,
    ) -> PraxisResult<CommandIntentPlan> {
        self.note_runtime_command_activity(thread_id, RuntimeCommandActivity::WorkerStartedCommand)
            .await;
        let intent = classify_mutating_tool(tool_name);
        let now = Utc::now();
        let action = vec![
            format!("tool:{tool_name}"),
            arguments_fingerprint_source.to_string(),
        ];
        let (thread, task, profile) = {
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
            (thread, task, profile)
        };

        profile
            .validate_tool_intent(&intent)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let plan = CommandIntentPlan {
            plan_id: format!("intent-plan-{}", Uuid::new_v4()),
            task_id: task.task_id,
            thread_id,
            intent: intent.kind,
            confidence: intent.confidence,
            command_fingerprint: action_fingerprint(&action, thread.cwd.as_path(), intent.kind),
            command: action,
            cwd: thread.cwd,
            required_capabilities,
            required_resources: intent.required_resources,
            side_effects: intent.side_effects,
            risk_level: intent.risk_level,
            status: CommandIntentPlanStatus::Pending,
            consumed_by_ticket_id: None,
            created_at: now,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
        };
        self.insert_intent_plan(&plan).await;
        self.persist_intent_plan_snapshot(&plan).await;
        self.record_event(
            "mutating_tool_intent_preflight",
            Some(thread_id),
            Some(plan.task_id.clone()),
            None,
            json!({
                "plan_id": &plan.plan_id,
                "tool": tool_name,
                "intent": plan.intent.as_str(),
                "confidence": plan.confidence,
                "risk_level": &plan.risk_level,
                "status": format!("{:?}", plan.status),
                "expires_at": plan.expires_at.to_rfc3339(),
                "required_capabilities": &plan.required_capabilities,
                "required_resources": plan
                    .required_resources
                    .iter()
                    .map(ResourceRequirement::key)
                    .collect::<Vec<_>>(),
            }),
        )
        .await;
        Ok(plan)
    }

    pub(super) fn find_matching_intent_plan_locked<'a>(
        state: &'a AgentOsState,
        thread_id: ThreadId,
        task_id: &str,
        intent: ActionIntentKind,
        command_fingerprint: &str,
        cwd: &Path,
    ) -> Option<&'a CommandIntentPlan> {
        let cwd = normalize_path_for_scope(cwd);
        let now = Utc::now();
        state
            .intent_plans
            .values()
            .filter(|plan| plan.status == CommandIntentPlanStatus::Pending)
            .filter(|plan| plan.expires_at > now)
            .filter(|plan| plan.thread_id == thread_id)
            .filter(|plan| plan.task_id == task_id)
            .filter(|plan| plan.intent == intent)
            .filter(|plan| plan.command_fingerprint == command_fingerprint)
            .filter(|plan| normalize_path_for_scope(plan.cwd.as_path()) == cwd)
            .max_by_key(|plan| plan.created_at)
    }

    pub(super) async fn insert_intent_plan(&self, plan: &CommandIntentPlan) {
        let superseded_plans = {
            let mut state = self.state.write().await;
            let normalized_cwd = normalize_path_for_scope(plan.cwd.as_path());
            let superseded_plans = state
                .intent_plans
                .values_mut()
                .filter(|existing| existing.status == CommandIntentPlanStatus::Pending)
                .filter(|existing| existing.thread_id == plan.thread_id)
                .filter(|existing| existing.task_id == plan.task_id.as_str())
                .filter(|existing| existing.intent == plan.intent)
                .filter(|existing| {
                    existing.command_fingerprint == plan.command_fingerprint.as_str()
                })
                .filter(|existing| {
                    normalize_path_for_scope(existing.cwd.as_path()) == normalized_cwd
                })
                .map(|existing| {
                    existing.status = CommandIntentPlanStatus::Rejected;
                    existing.clone()
                })
                .collect::<Vec<_>>();
            state
                .intent_plans
                .insert(plan.plan_id.clone(), plan.clone());
            superseded_plans
        };
        for superseded in superseded_plans {
            self.persist_intent_plan_snapshot(&superseded).await;
            self.record_event(
                "command_intent_plan_superseded",
                Some(superseded.thread_id),
                Some(superseded.task_id.clone()),
                None,
                json!({
                    "plan_id": &superseded.plan_id,
                    "replaced_by_plan_id": &plan.plan_id,
                    "intent": superseded.intent.as_str(),
                }),
            )
            .await;
        }
    }

    pub(super) async fn expire_intent_plans(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            state
                .intent_plans
                .values_mut()
                .filter(|plan| plan.status == CommandIntentPlanStatus::Pending)
                .filter(|plan| plan.expires_at <= now)
                .map(|plan| {
                    plan.status = CommandIntentPlanStatus::Expired;
                    plan.clone()
                })
                .collect::<Vec<_>>()
        };
        for plan in expired {
            self.persist_intent_plan_snapshot(&plan).await;
            self.record_event(
                "command_intent_plan_expired",
                Some(plan.thread_id),
                Some(plan.task_id.clone()),
                None,
                json!({
                    "plan_id": &plan.plan_id,
                    "intent": plan.intent.as_str(),
                    "expires_at": plan.expires_at.to_rfc3339(),
                }),
            )
            .await;
        }
    }
}

use super::*;

impl AgentOs {
    pub(crate) async fn preflight_command_intent(
        &self,
        thread_id: ThreadId,
        command: &[String],
        cwd: &Path,
    ) -> PraxisResult<CommandIntentPlan> {
        self.note_worker_started_command(thread_id).await;
        let intent = classify_command(command, cwd);
        let now = Utc::now();
        let (thread, task, profile) = self.state.write().await.resolve_thread_context(
            thread_id,
            "side-effectful action rejected: thread has no current task_id",
        )?;

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
}

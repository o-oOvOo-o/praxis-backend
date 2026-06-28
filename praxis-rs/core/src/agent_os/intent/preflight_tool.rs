use super::*;

impl AgentOs {
    pub(crate) async fn preflight_mutating_tool_intent(
        &self,
        thread_id: ThreadId,
        tool_name: &str,
        arguments_fingerprint_source: &str,
    ) -> PraxisResult<CommandIntentPlan> {
        self.note_worker_started_command(thread_id).await;
        let intent = classify_mutating_tool(tool_name);
        let now = Utc::now();
        let action = vec![
            format!("tool:{tool_name}"),
            arguments_fingerprint_source.to_string(),
        ];
        let (thread, task, profile) = self.state.write().await.resolve_thread_context(
            thread_id,
            "side-effectful tool rejected: thread has no current task_id",
        )?;

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
}

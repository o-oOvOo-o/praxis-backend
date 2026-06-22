use super::*;

impl AgentOs {
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
        let context = self
            .resolve_ticket_issue_context(
                thread_id,
                "side-effectful tool rejected: thread has no current task_id",
                "run side-effectful tools",
                intent.kind,
                None,
                Some(&action),
                None,
                now,
            )
            .await?;

        context
            .profile
            .validate_tool_intent(&intent)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = context.profile.capability_names_for_action(&intent);
        validate_task_action_contract(
            &context.task,
            &required_capabilities,
            &intent.required_resources,
        )?;

        let (ticket, plan_snapshot_result) = self
            .issue_validated_ticket(
                thread_id,
                context,
                &intent,
                required_capabilities,
                "tool ticket rejected: tool has no matching AgentOS intent preflight plan",
                "tool ticket rejected",
                "tool ticket",
                now,
            )
            .await?;

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
}

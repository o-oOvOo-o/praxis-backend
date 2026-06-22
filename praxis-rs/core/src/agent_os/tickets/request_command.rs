use super::*;

impl AgentOs {
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
        let context = self
            .resolve_ticket_issue_context(
                thread_id,
                "side-effectful action rejected: thread has no current task_id",
                "run side-effectful actions",
                intent.kind,
                Some(action_fingerprint(command, cwd, intent.kind)),
                None,
                Some(cwd.to_path_buf()),
                now,
            )
            .await?;

        context
            .profile
            .validate_command_intent(&intent, command, cwd)
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
                "execution ticket rejected: command has no matching AgentOS intent preflight plan",
                "execution ticket rejected",
                "execution ticket",
                now,
            )
            .await?;

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
}

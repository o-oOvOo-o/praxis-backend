use super::context::TicketIssueContext;
use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn issue_validated_ticket(
        &self,
        thread_id: ThreadId,
        context: TicketIssueContext,
        intent: &ActionIntent,
        required_capabilities: Vec<String>,
        missing_plan_message: &str,
        consume_rejection_prefix: &str,
        consume_ticket_label: &str,
        now: chrono::DateTime<Utc>,
    ) -> PraxisResult<(ExecutionTicket, CommandIntentPlan)> {
        let intent_plan_id = context
            .intent_plan_id
            .ok_or_else(|| PraxisErr::UnsupportedOperation(missing_plan_message.to_string()))?;
        let ticket_intent_plan_id = intent_plan_id.clone();
        let task_id = context.task.task_id.clone();

        let lease_ids = match self
            .acquire_required_leases(
                thread_id,
                task_id.as_str(),
                context.thread.priority.max(context.task.priority),
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
            task_id: context.task.task_id,
            thread_id,
            coordination_scope: context.thread.coordination_scope,
            allowed_intent: intent.kind,
            intent_plan_id: Some(intent_plan_id),
            command_fingerprint: context.command_fingerprint,
            cwd: context.cwd,
            risk_level: intent.risk_level.clone(),
            capabilities: required_capabilities,
            lease_ids,
            file_scopes: context.profile.path_scopes.allow.clone(),
            token_budget: context.task.token_budget,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
            fencing_token: context.coordinator_fencing,
            coordinator_epoch: context.coordinator_epoch,
            created_at: now,
        };

        let plan_snapshot_result = {
            let mut state = self.state.write().await;
            state.consume_intent_plan_for_ticket(
                &ticket,
                ticket_intent_plan_id.as_str(),
                consume_rejection_prefix,
                consume_ticket_label,
                now,
            )
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
        Ok((ticket, plan_snapshot_result))
    }
}

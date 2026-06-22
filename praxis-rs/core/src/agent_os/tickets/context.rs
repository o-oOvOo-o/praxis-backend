use super::*;

pub(in crate::agent_os) struct TicketIssueContext {
    pub(in crate::agent_os) thread: ThreadRegistryEntry,
    pub(in crate::agent_os) task: TaskRecord,
    pub(in crate::agent_os) profile: CapabilityProfile,
    pub(in crate::agent_os) coordinator_epoch: u64,
    pub(in crate::agent_os) coordinator_fencing: u64,
    pub(in crate::agent_os) intent_plan_id: Option<String>,
    pub(in crate::agent_os) command_fingerprint: String,
    pub(in crate::agent_os) cwd: PathBuf,
}

impl AgentOs {
    pub(in crate::agent_os) async fn resolve_ticket_issue_context(
        &self,
        thread_id: ThreadId,
        missing_task_message: &str,
        coordinator_reason: &str,
        intent_kind: ActionIntentKind,
        command_fingerprint: Option<String>,
        fingerprint_action: Option<&[String]>,
        cwd: Option<PathBuf>,
        now: chrono::DateTime<Utc>,
    ) -> PraxisResult<TicketIssueContext> {
        let mut state = self.state.write().await;
        let (thread, task, profile) =
            state.resolve_thread_context(thread_id, missing_task_message)?;
        let active = if thread.rank == COORDINATOR_RANK {
            Self::claim_or_renew_active_coordinator_locked(
                &mut state,
                &thread,
                now,
                Some(coordinator_reason),
            )?
        } else {
            state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned()
        };
        let cwd = cwd.unwrap_or_else(|| thread.cwd.clone());
        let command_fingerprint = match command_fingerprint {
            Some(command_fingerprint) => command_fingerprint,
            None => {
                let action = fingerprint_action.ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(
                        "ticket issue context missing fingerprint action".to_string(),
                    )
                })?;
                action_fingerprint(action, cwd.as_path(), intent_kind)
            }
        };
        let intent_plan_id = state
            .find_matching_intent_plan(
                thread_id,
                task.task_id.as_str(),
                intent_kind,
                command_fingerprint.as_str(),
                cwd.as_path(),
            )
            .map(|plan| plan.plan_id.clone());
        Ok(TicketIssueContext {
            thread,
            task,
            profile,
            coordinator_epoch: active.as_ref().map(|value| value.epoch).unwrap_or(0),
            coordinator_fencing: active
                .as_ref()
                .map(|value| value.fencing_token)
                .unwrap_or(0),
            intent_plan_id,
            command_fingerprint,
            cwd,
        })
    }
}

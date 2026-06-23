use super::*;

impl AgentOs {
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
}

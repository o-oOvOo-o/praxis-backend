use super::*;

impl AgentOs {
    /// Claim runtime commands for injection into the worker's next turn.
    ///
    /// This is the runtime-lifecycle path, not a model-driven tool call: the
    /// worker does not need to remember to call `poll_runtime_commands` before
    /// receiving its assignment. Claiming a command marks it as consumed by the
    /// runtime. AssignTask commands move directly into Executing so they are
    /// not re-injected on later turns; other command types are Acked and remain
    /// visible to explicit status tools. Already-Acked commands are not claimed
    /// again, which prevents non-AssignTask commands from being injected every
    /// turn forever.
    pub(crate) async fn claim_runtime_commands_for_turn(
        &self,
        thread_id: ThreadId,
    ) -> PraxisResult<Vec<RuntimeCommandRecord>> {
        let now = Utc::now();
        let ttl = AgentOsPolicy::get().ticket_ttl();
        let (claimed, changed_commands, changed_tasks, changed_threads) = {
            let mut state = self.state.write().await;
            if !state.threads.contains_key(&thread_id) {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS thread `{thread_id}`"
                )));
            }
            let active = state.active_coordinator_for_thread(thread_id).cloned();
            let mut claimed = Vec::new();
            let mut changed_commands = Vec::new();
            let mut changed_tasks = Vec::new();
            let mut changed_threads = Vec::new();
            let command_ids = state
                .runtime_commands
                .iter()
                .filter_map(|(command_id, command)| {
                    if command.to_thread_id != thread_id {
                        return None;
                    }
                    if !command.status.is_unclaimed() {
                        return None;
                    }
                    Some(command_id.clone())
                })
                .collect::<Vec<_>>();

            for command_id in command_ids {
                let Some(command_snapshot) = state.runtime_commands.get(&command_id).cloned()
                else {
                    continue;
                };
                let next_status = command_snapshot.claim_status(active.as_ref(), now);

                let Some(command) = state.runtime_commands.get_mut(&command_id) else {
                    continue;
                };
                command.status = next_status;
                command.updated_at = now;
                command.expires_at = now + ttl;
                let updated_command = command.clone();
                changed_commands.push(updated_command.clone());

                if next_status == RuntimeCommandStatus::Executing {
                    let snapshots = state.apply_assign_runtime_status(&updated_command, now, true);
                    if let Some(task) = snapshots.task {
                        changed_tasks.push(task);
                    }
                    if let Some(thread) = snapshots.thread {
                        changed_threads.push(thread);
                    }
                }

                if next_status.is_turn_claimed() {
                    claimed.push(updated_command);
                }
            }
            (claimed, changed_commands, changed_tasks, changed_threads)
        };

        for command in &changed_commands {
            self.persist_runtime_command_snapshot(command).await;
        }
        for task in &changed_tasks {
            self.persist_task_snapshot(task).await;
        }
        for thread in &changed_threads {
            self.persist_thread_snapshot(thread).await;
        }
        if !changed_commands.is_empty() {
            self.record_event(
                "runtime_commands_claimed_for_turn",
                Some(thread_id),
                None,
                None,
                json!({
                    "claimed_commands": claimed.iter().map(|command| json!({
                        "command_id": &command.command_id,
                        "command_type": command.command_type.as_str(),
                        "task_id": &command.task_id,
                        "status": format!("{:?}", command.status),
                    })).collect::<Vec<_>>(),
                    "changed_commands": changed_commands.iter().map(|command| json!({
                        "command_id": &command.command_id,
                        "status": format!("{:?}", command.status),
                    })).collect::<Vec<_>>(),
                }),
            )
            .await;
        }

        Ok(claimed)
    }
}

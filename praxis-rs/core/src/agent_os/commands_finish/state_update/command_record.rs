use super::super::*;
use super::finish_state::ManagedCommandFinishState;
use super::process::mark_command_process_finished;
use super::thread_task::{
    update_task_after_finished_command, update_thread_after_finished_command,
};

impl AgentOs {
    pub(in crate::agent_os::commands_finish) async fn mark_managed_command_finished_in_state(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
    ) -> PraxisResult<ManagedCommandFinishState> {
        let now = Utc::now();
        let (command, thread_snapshot, task_snapshot, lease_ids) = {
            let mut state = self.state.write().await;
            let (command_snapshot, process_ref) =
                finish_command_record(&mut state, command_id, exit_code, now)?;
            mark_command_process_finished(&mut state, process_ref, now);
            let has_active_runtime_command = state.has_active_assign_runtime_command(
                command_snapshot.thread_id,
                command_snapshot.task_id.as_str(),
            );
            let lease_ids = command_snapshot.lease_ids.clone();
            let thread_snapshot = update_thread_after_finished_command(
                &mut state,
                command_id,
                &command_snapshot,
                has_active_runtime_command,
                now,
            );
            let task_snapshot = update_task_after_finished_command(
                &mut state,
                &command_snapshot,
                has_active_runtime_command,
                now,
            );
            (command_snapshot, thread_snapshot, task_snapshot, lease_ids)
        };

        Ok(ManagedCommandFinishState {
            command,
            thread_snapshot,
            task_snapshot,
            lease_ids,
        })
    }
}

fn finish_command_record(
    state: &mut crate::agent_os::state::AgentOsState,
    command_id: &str,
    exit_code: Option<i32>,
    now: chrono::DateTime<Utc>,
) -> PraxisResult<(CommandRecord, Option<(i32, Option<String>)>)> {
    let command = state.commands.get_mut(command_id).ok_or_else(|| {
        PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
    })?;
    command.ended_at = Some(now);
    command.exit_code = exit_code;
    let process_ref = command
        .process_id
        .map(|process_id| (process_id, command.runtime_owner_id.clone()));
    Ok((command.clone(), process_ref))
}

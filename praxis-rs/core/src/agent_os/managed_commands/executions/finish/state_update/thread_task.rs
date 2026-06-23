use super::super::*;

pub(super) fn update_thread_after_finished_command(
    state: &mut crate::agent_os::state::AgentOsState,
    command_id: &str,
    command: &CommandRecord,
    has_active_runtime_command: bool,
    now: chrono::DateTime<Utc>,
) -> Option<ThreadRegistryEntry> {
    let thread = state.threads.get_mut(&command.thread_id)?;
    if thread.current_command_id.as_deref() == Some(command_id) {
        thread.current_command_id = None;
    }
    if has_active_runtime_command {
        thread.current_task_id = Some(command.task_id.clone());
        if !matches!(
            thread.state,
            ThreadRuntimeState::WaitingForLease
                | ThreadRuntimeState::WaitingForCoordinator
                | ThreadRuntimeState::Stopping
                | ThreadRuntimeState::Stopped
                | ThreadRuntimeState::Failed
                | ThreadRuntimeState::Completed
        ) {
            thread.state = ThreadRuntimeState::Running;
        }
    } else {
        thread.state = ThreadRuntimeState::Idle;
    }
    thread.heartbeat_at = now;
    Some(thread.clone())
}

pub(super) fn update_task_after_finished_command(
    state: &mut crate::agent_os::state::AgentOsState,
    command: &CommandRecord,
    has_active_runtime_command: bool,
    now: chrono::DateTime<Utc>,
) -> Option<TaskRecord> {
    let task = state.tasks.get_mut(&command.task_id)?;
    if !matches!(
        task.status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    ) {
        task.status = if has_active_runtime_command {
            TaskStatus::Running
        } else {
            TaskStatus::Assigned
        };
    }
    task.updated_at = now;
    Some(task.clone())
}

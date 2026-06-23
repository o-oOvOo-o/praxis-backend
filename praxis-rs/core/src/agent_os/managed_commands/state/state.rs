use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;

use crate::agent_os::records::ActiveCoordinatorLease;
use crate::agent_os::records::RuntimeCommandRecord;
use crate::agent_os::records::RuntimeCommandType;
use crate::agent_os::records::TaskRecord;
use crate::agent_os::records::ThreadRegistryEntry;
use crate::agent_os::state::AgentOsState;

#[derive(Default)]
pub(in crate::agent_os) struct AssignRuntimeStatusSnapshots {
    pub(in crate::agent_os) thread: Option<ThreadRegistryEntry>,
    pub(in crate::agent_os) task: Option<TaskRecord>,
}

impl AgentOsState {
    pub(in crate::agent_os) fn active_coordinator_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> Option<&ActiveCoordinatorLease> {
        let thread = self.threads.get(&thread_id)?;
        self.active_coordinators
            .get(thread.coordination_scope.as_str())
    }

    pub(in crate::agent_os) fn apply_assign_runtime_status(
        &mut self,
        command: &RuntimeCommandRecord,
        now: DateTime<Utc>,
        set_current_command: bool,
    ) -> AssignRuntimeStatusSnapshots {
        if command.command_type != RuntimeCommandType::AssignTask {
            return AssignRuntimeStatusSnapshots::default();
        }
        let Some(task_id) = command.task_id.as_deref() else {
            return AssignRuntimeStatusSnapshots::default();
        };

        let mut snapshots = AssignRuntimeStatusSnapshots::default();
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = command.status.assign_task_status();
            task.updated_at = now;
            snapshots.task = Some(task.clone());
        }
        if let Some(thread) = self.threads.get_mut(&command.to_thread_id) {
            if command.status.clears_assigned_task() {
                if thread.current_task_id.as_deref() == Some(task_id) {
                    thread.current_task_id = None;
                }
            } else {
                thread.current_task_id = Some(task_id.to_string());
            }
            if set_current_command
                && command.status == crate::agent_os::records::RuntimeCommandStatus::Executing
            {
                thread.current_command_id = Some(command.command_id.clone());
            }
            thread.state = command.status.assign_thread_state();
            thread.heartbeat_at = now;
            snapshots.thread = Some(thread.clone());
        }
        snapshots
    }

    pub(in crate::agent_os) fn has_active_assign_runtime_command(
        &self,
        thread_id: ThreadId,
        task_id: &str,
    ) -> bool {
        self.runtime_commands.values().any(|command| {
            command.to_thread_id == thread_id
                && command.command_type == RuntimeCommandType::AssignTask
                && command.task_id.as_deref() == Some(task_id)
                && command.status.is_live()
        })
    }
}

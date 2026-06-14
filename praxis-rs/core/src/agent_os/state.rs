use super::model::*;
use praxis_protocol::ThreadId;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct AgentOsState {
    pub(super) threads: HashMap<ThreadId, ThreadRegistryEntry>,
    pub(super) profiles: HashMap<String, CapabilityProfile>,
    pub(super) tasks: HashMap<String, TaskRecord>,
    pub(super) leases: HashMap<String, ResourceLease>,
    pub(super) tickets: HashMap<String, ExecutionTicket>,
    pub(super) intent_plans: HashMap<String, CommandIntentPlan>,
    pub(super) commands: HashMap<String, CommandRecord>,
    pub(super) processes: HashMap<String, ManagedProcessRecord>,
    pub(super) artifacts: HashMap<String, ArtifactRecord>,
    pub(super) worker_requests: HashMap<String, WorkerRequestRecord>,
    pub(super) runtime_commands: HashMap<String, RuntimeCommandRecord>,
    pub(super) events: Vec<EventLedgerEntry>,
    pub(super) active_coordinators: HashMap<String, ActiveCoordinatorLease>,
    pub(super) fencing_counter: u64,
    pub(super) coordinator_epoch: u64,
}

pub(super) fn has_active_assign_runtime_command_locked(
    state: &AgentOsState,
    thread_id: ThreadId,
    task_id: &str,
) -> bool {
    state.runtime_commands.values().any(|command| {
        command.to_thread_id == thread_id
            && command.command_type == RuntimeCommandType::AssignTask
            && command.task_id.as_deref() == Some(task_id)
            && matches!(
                command.status,
                RuntimeCommandStatus::Pending
                    | RuntimeCommandStatus::Acked
                    | RuntimeCommandStatus::Executing
            )
    })
}

use super::records::*;
use praxis_protocol::ThreadId;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct AgentOsState {
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

use super::options::AgentOsSnapshotOptions;
use super::summaries::{
    AgentOsArtifactSummary, AgentOsIntentPlanSummary, AgentOsLeaseSummary,
    AgentOsWorkerRequestSummary, RuntimeCommandSummary,
};
use crate::agent_os::WorkerRequestStatus;
use crate::agent_os::state::AgentOsState;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsSnapshot {
    pub(crate) sequence: u64,
    pub(crate) leases: Vec<AgentOsLeaseSummary>,
    pub(crate) recent_artifacts: Vec<AgentOsArtifactSummary>,
    pub(crate) pending_worker_requests: Vec<AgentOsWorkerRequestSummary>,
    pub(crate) pending_runtime_commands: Vec<RuntimeCommandSummary>,
    pub(crate) recent_intent_plans: Vec<AgentOsIntentPlanSummary>,
}

impl AgentOsSnapshot {
    pub(in crate::agent_os) fn from_state(
        state: &AgentOsState,
        options: AgentOsSnapshotOptions,
        sequence: impl FnOnce() -> u64,
    ) -> Self {
        let mut artifacts = state.artifacts.values().cloned().collect::<Vec<_>>();
        artifacts.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut worker_requests = state.worker_requests.values().cloned().collect::<Vec<_>>();
        worker_requests.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut runtime_commands = state.runtime_commands.values().cloned().collect::<Vec<_>>();
        runtime_commands.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut intent_plans = state.intent_plans.values().cloned().collect::<Vec<_>>();
        intent_plans.sort_by(|left, right| right.created_at.cmp(&left.created_at));

        Self {
            sequence: sequence(),
            leases: state
                .leases
                .values()
                .cloned()
                .map(AgentOsLeaseSummary::from)
                .collect(),
            recent_artifacts: artifacts
                .into_iter()
                .take(options.recent_artifact_limit)
                .map(AgentOsArtifactSummary::from)
                .collect(),
            pending_worker_requests: worker_requests
                .into_iter()
                .filter(|request| request.status == WorkerRequestStatus::Pending)
                .take(options.pending_worker_request_limit)
                .map(AgentOsWorkerRequestSummary::from)
                .collect(),
            pending_runtime_commands: runtime_commands
                .into_iter()
                .filter(|command| command.status.is_live())
                .take(options.pending_runtime_command_limit)
                .map(RuntimeCommandSummary::from)
                .collect(),
            recent_intent_plans: intent_plans
                .into_iter()
                .take(options.recent_intent_plan_limit)
                .map(AgentOsIntentPlanSummary::from)
                .collect(),
        }
    }

    pub(crate) fn no_pending_work(&self) -> bool {
        self.leases.is_empty()
            && self.pending_worker_requests.is_empty()
            && self.pending_runtime_commands.is_empty()
    }
}

use super::options::AgentOsSnapshotOptions;
use super::summaries::AgentOsArtifactSummary;
use super::summaries::AgentOsIntentPlanSummary;
use super::summaries::AgentOsLeaseSummary;
use super::summaries::AgentOsWorkerRequestSummary;
use super::summaries::RuntimeCommandSummary;
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
        let recent_artifacts = newest_by_created_at(
            state.artifacts.values(),
            options.recent_artifact_limit,
            |artifact| artifact.created_at,
        );
        let pending_worker_requests = newest_by_created_at(
            state
                .worker_requests
                .values()
                .filter(|request| request.status == WorkerRequestStatus::Pending),
            options.pending_worker_request_limit,
            |request| request.created_at,
        );
        let pending_runtime_commands = newest_by_created_at(
            state
                .runtime_commands
                .values()
                .filter(|command| command.status.is_live()),
            options.pending_runtime_command_limit,
            |command| command.created_at,
        );
        let recent_intent_plans = newest_by_created_at(
            state.intent_plans.values(),
            options.recent_intent_plan_limit,
            |plan| plan.created_at,
        );

        Self {
            sequence: sequence(),
            leases: state
                .leases
                .values()
                .cloned()
                .map(AgentOsLeaseSummary::from)
                .collect(),
            recent_artifacts: recent_artifacts
                .into_iter()
                .map(AgentOsArtifactSummary::from)
                .collect(),
            pending_worker_requests: pending_worker_requests
                .into_iter()
                .map(AgentOsWorkerRequestSummary::from)
                .collect(),
            pending_runtime_commands: pending_runtime_commands
                .into_iter()
                .map(RuntimeCommandSummary::from)
                .collect(),
            recent_intent_plans: recent_intent_plans
                .into_iter()
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

fn newest_by_created_at<'a, T, I, F>(values: I, limit: usize, created_at: F) -> Vec<T>
where
    T: Clone + 'a,
    I: Iterator<Item = &'a T>,
    F: Fn(&T) -> chrono::DateTime<chrono::Utc>,
{
    if limit == 0 {
        return Vec::new();
    }
    let mut values = values.collect::<Vec<_>>();
    if values.len() > limit {
        values.select_nth_unstable_by(limit, |left, right| {
            created_at(right).cmp(&created_at(left))
        });
        values.truncate(limit);
    }
    values.sort_unstable_by(|left, right| created_at(right).cmp(&created_at(left)));
    values.into_iter().take(limit).cloned().collect::<Vec<_>>()
}

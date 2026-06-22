#[derive(Clone, Copy, Debug)]
pub(crate) struct AgentOsSnapshotOptions {
    pub(crate) recent_artifact_limit: usize,
    pub(crate) pending_worker_request_limit: usize,
    pub(crate) pending_runtime_command_limit: usize,
    pub(crate) recent_intent_plan_limit: usize,
}

impl Default for AgentOsSnapshotOptions {
    fn default() -> Self {
        Self {
            recent_artifact_limit: 20,
            pending_worker_request_limit: 20,
            pending_runtime_command_limit: 20,
            recent_intent_plan_limit: 20,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AgentOsEventQuery {
    pub(crate) since_sequence: u64,
    pub(crate) limit: usize,
}

impl Default for AgentOsEventQuery {
    fn default() -> Self {
        Self {
            since_sequence: 0,
            limit: 256,
        }
    }
}

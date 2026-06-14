use super::*;
use crate::util::truncate_to_char_boundary;
use serde::Serialize;

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

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsEventBatch {
    pub(crate) current_sequence: u64,
    pub(crate) events: Vec<EventLedgerEntry>,
}

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
    pub(crate) fn no_pending_work(&self) -> bool {
        self.leases.is_empty()
            && self.pending_worker_requests.is_empty()
            && self.pending_runtime_commands.is_empty()
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsLeaseSummary {
    lease_id: String,
    resource_type: String,
    scope: String,
    mode: String,
    owner_thread_id: String,
    task_id: String,
    priority: i32,
    expires_at: Option<String>,
}

impl From<ResourceLease> for AgentOsLeaseSummary {
    fn from(lease: ResourceLease) -> Self {
        Self {
            lease_id: lease.lease_id,
            resource_type: lease.resource_type,
            scope: lease.scope,
            mode: format!("{:?}", lease.mode),
            owner_thread_id: lease.owner_thread_id.to_string(),
            task_id: lease.task_id,
            priority: lease.priority,
            expires_at: lease.expires_at.map(|expires_at| expires_at.to_rfc3339()),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsArtifactSummary {
    artifact_id: String,
    task_id: String,
    owner_thread_id: String,
    artifact_type: String,
    uri: String,
    summary: String,
    blob_persisted: bool,
    blob_bytes: Option<u64>,
    blob_path: Option<String>,
    created_at: String,
}

impl From<ArtifactRecord> for AgentOsArtifactSummary {
    fn from(artifact: ArtifactRecord) -> Self {
        let mut summary = artifact.summary;
        truncate_to_char_boundary(&mut summary, 500);
        let blob = artifact.metadata.get("blob");
        Self {
            artifact_id: artifact.artifact_id,
            task_id: artifact.task_id,
            owner_thread_id: artifact.owner_thread_id.to_string(),
            artifact_type: format!("{:?}", artifact.artifact_type),
            uri: artifact.uri,
            summary,
            blob_persisted: blob
                .and_then(|value| value.get("blob_persisted"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            blob_bytes: blob
                .and_then(|value| value.get("blob_bytes"))
                .and_then(|value| value.as_u64()),
            blob_path: blob
                .and_then(|value| value.get("blob_path"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
            created_at: artifact.created_at.to_rfc3339(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsWorkerRequestSummary {
    request_id: String,
    request_type: String,
    thread_id: String,
    task_id: Option<String>,
    blocking: bool,
    status: String,
    reason: String,
    requested_resource: Option<String>,
    artifact_refs: Vec<String>,
    created_at: String,
}

impl From<WorkerRequestRecord> for AgentOsWorkerRequestSummary {
    fn from(request: WorkerRequestRecord) -> Self {
        let mut reason = request.reason;
        truncate_to_char_boundary(&mut reason, 500);
        Self {
            request_id: request.request_id,
            request_type: request.request_type,
            thread_id: request.thread_id.to_string(),
            task_id: request.task_id,
            blocking: request.blocking,
            status: format!("{:?}", request.status),
            reason,
            requested_resource: request.requested_resource,
            artifact_refs: request.artifact_refs,
            created_at: request.created_at.to_rfc3339(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RuntimeCommandSummary {
    command_id: String,
    from_thread_id: String,
    to_thread_id: String,
    task_id: Option<String>,
    command_type: String,
    status: String,
    coordinator_epoch: u64,
    fencing_token: u64,
    created_at: String,
    expires_at: String,
}

impl From<RuntimeCommandRecord> for RuntimeCommandSummary {
    fn from(command: RuntimeCommandRecord) -> Self {
        Self {
            command_id: command.command_id,
            from_thread_id: command.from_thread_id.to_string(),
            to_thread_id: command.to_thread_id.to_string(),
            task_id: command.task_id,
            command_type: format!("{:?}", command.command_type),
            status: format!("{:?}", command.status),
            coordinator_epoch: command.coordinator_epoch,
            fencing_token: command.fencing_token,
            created_at: command.created_at.to_rfc3339(),
            expires_at: command.expires_at.to_rfc3339(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsIntentPlanSummary {
    plan_id: String,
    task_id: String,
    thread_id: String,
    intent: String,
    confidence: f32,
    command_fingerprint: String,
    cwd: String,
    required_capabilities: Vec<String>,
    required_resources: Vec<String>,
    risk_level: String,
    status: String,
    consumed_by_ticket_id: Option<String>,
    created_at: String,
    expires_at: String,
}

impl From<CommandIntentPlan> for AgentOsIntentPlanSummary {
    fn from(plan: CommandIntentPlan) -> Self {
        Self {
            plan_id: plan.plan_id,
            task_id: plan.task_id,
            thread_id: plan.thread_id.to_string(),
            intent: format!("{:?}", plan.intent),
            confidence: plan.confidence,
            command_fingerprint: plan.command_fingerprint,
            cwd: plan.cwd.display().to_string(),
            required_capabilities: plan.required_capabilities,
            required_resources: plan
                .required_resources
                .iter()
                .map(ResourceRequirement::key)
                .collect(),
            risk_level: plan.risk_level,
            status: format!("{:?}", plan.status),
            consumed_by_ticket_id: plan.consumed_by_ticket_id,
            created_at: plan.created_at.to_rfc3339(),
            expires_at: plan.expires_at.to_rfc3339(),
        }
    }
}

impl AgentOs {
    pub(crate) async fn events_since(&self, query: AgentOsEventQuery) -> AgentOsEventBatch {
        let state = self.state.read().await;
        let mut events = state
            .events
            .iter()
            .filter(|event| event.sequence > query.since_sequence)
            .cloned()
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.sequence);
        if events.len() > query.limit {
            let drop_count = events.len() - query.limit;
            events.drain(0..drop_count);
        }
        AgentOsEventBatch {
            current_sequence: self.change_sequence(),
            events,
        }
    }

    pub(crate) async fn snapshot(&self, options: AgentOsSnapshotOptions) -> AgentOsSnapshot {
        self.expire_tickets().await;
        self.expire_leases().await;
        self.expire_runtime_commands().await;
        self.expire_intent_plans().await;

        let state = self.state.read().await;
        let mut artifacts = state.artifacts.values().cloned().collect::<Vec<_>>();
        artifacts.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut worker_requests = state.worker_requests.values().cloned().collect::<Vec<_>>();
        worker_requests.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut runtime_commands = state.runtime_commands.values().cloned().collect::<Vec<_>>();
        runtime_commands.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut intent_plans = state.intent_plans.values().cloned().collect::<Vec<_>>();
        intent_plans.sort_by(|left, right| right.created_at.cmp(&left.created_at));

        AgentOsSnapshot {
            sequence: self.change_sequence(),
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
                .filter(|command| {
                    matches!(
                        command.status,
                        RuntimeCommandStatus::Pending
                            | RuntimeCommandStatus::Acked
                            | RuntimeCommandStatus::Executing
                    )
                })
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
}

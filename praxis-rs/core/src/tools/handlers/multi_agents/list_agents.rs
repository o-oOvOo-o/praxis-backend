use super::*;
use crate::agent::control::ListedAgent;
use crate::agent_os::ArtifactRecord;
use crate::agent_os::CommandIntentPlan;
use crate::agent_os::ResourceLease;
use crate::agent_os::RuntimeCommandRecord;
use crate::agent_os::RuntimeCommandStatus;
use crate::agent_os::WorkerRequestRecord;
use crate::agent_os::WorkerRequestStatus;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = ListAgentsResult;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: ListAgentsArgs = parse_arguments(&arguments)?;
        session
            .services
            .agent_control
            .register_session_root(session.conversation_id, &turn.session_source);
        let agents = session
            .services
            .agent_control
            .list_agents(
                session.conversation_id,
                &turn.session_source,
                args.path_prefix.as_deref(),
            )
            .await
            .map_err(collab_spawn_error)?;

        let mut artifacts = session.services.agent_os.query_artifacts().await;
        artifacts.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut worker_requests = session.services.agent_os.query_worker_requests().await;
        worker_requests.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut runtime_commands = session.services.agent_os.query_runtime_commands().await;
        runtime_commands.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut intent_plans = session.services.agent_os.query_intent_plans().await;
        intent_plans.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let agent_os = AgentOsSnapshot {
            leases: session
                .services
                .agent_os
                .query_leases()
                .await
                .into_iter()
                .map(AgentOsLeaseSummary::from)
                .collect(),
            recent_artifacts: artifacts
                .into_iter()
                .take(20)
                .map(AgentOsArtifactSummary::from)
                .collect(),
            pending_worker_requests: worker_requests
                .into_iter()
                .filter(|request| request.status == WorkerRequestStatus::Pending)
                .take(20)
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
                .take(20)
                .map(AgentOsRuntimeCommandSummary::from)
                .collect(),
            recent_intent_plans: intent_plans
                .into_iter()
                .take(20)
                .map(AgentOsIntentPlanSummary::from)
                .collect(),
        };

        Ok(ListAgentsResult { agents, agent_os })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListAgentsArgs {
    path_prefix: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAgentsResult {
    agents: Vec<ListedAgent>,
    agent_os: AgentOsSnapshot,
}

#[derive(Debug, Serialize)]
struct AgentOsSnapshot {
    leases: Vec<AgentOsLeaseSummary>,
    recent_artifacts: Vec<AgentOsArtifactSummary>,
    pending_worker_requests: Vec<AgentOsWorkerRequestSummary>,
    pending_runtime_commands: Vec<AgentOsRuntimeCommandSummary>,
    recent_intent_plans: Vec<AgentOsIntentPlanSummary>,
}

#[derive(Debug, Serialize)]
struct AgentOsLeaseSummary {
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

#[derive(Debug, Serialize)]
struct AgentOsArtifactSummary {
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
        if summary.len() > 500 {
            summary.truncate(500);
        }
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

#[derive(Debug, Serialize)]
struct AgentOsWorkerRequestSummary {
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
        if reason.len() > 500 {
            reason.truncate(500);
        }
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

#[derive(Debug, Serialize)]
struct AgentOsRuntimeCommandSummary {
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

impl From<RuntimeCommandRecord> for AgentOsRuntimeCommandSummary {
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

#[derive(Debug, Serialize)]
struct AgentOsIntentPlanSummary {
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
                .map(crate::agent_os::ResourceRequirement::key)
                .collect(),
            risk_level: plan.risk_level,
            status: format!("{:?}", plan.status),
            consumed_by_ticket_id: plan.consumed_by_ticket_id,
            created_at: plan.created_at.to_rfc3339(),
            expires_at: plan.expires_at.to_rfc3339(),
        }
    }
}

impl ToolOutput for ListAgentsResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "list_agents")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "list_agents")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "list_agents")
    }
}

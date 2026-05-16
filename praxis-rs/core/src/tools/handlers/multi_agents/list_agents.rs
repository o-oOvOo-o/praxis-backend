use super::*;
use crate::agent::control::ListedAgent;
use crate::agent_os::ActiveCoordinatorStatus;
use crate::agent_os::ArtifactRecord;
use crate::agent_os::ResourceLease;
use crate::agent_os::ThreadRegistryEntry;

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
        let agent_os = AgentOsSnapshot {
            active_coordinators: session
                .services
                .agent_os
                .query_active_coordinators()
                .await
                .into_iter()
                .map(AgentOsActiveCoordinatorSummary::from)
                .collect(),
            threads: session
                .services
                .agent_os
                .query_registry()
                .await
                .into_iter()
                .map(AgentOsThreadSummary::from)
                .collect(),
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
    active_coordinators: Vec<AgentOsActiveCoordinatorSummary>,
    threads: Vec<AgentOsThreadSummary>,
    leases: Vec<AgentOsLeaseSummary>,
    recent_artifacts: Vec<AgentOsArtifactSummary>,
}

#[derive(Debug, Serialize)]
struct AgentOsActiveCoordinatorSummary {
    coordination_scope: String,
    owner_thread_id: String,
    epoch: u64,
    fencing_token: u64,
    expires_at: String,
}

impl From<ActiveCoordinatorStatus> for AgentOsActiveCoordinatorSummary {
    fn from(status: ActiveCoordinatorStatus) -> Self {
        Self {
            coordination_scope: status.coordination_scope,
            owner_thread_id: status.owner_thread_id.to_string(),
            epoch: status.epoch,
            fencing_token: status.fencing_token,
            expires_at: status.expires_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AgentOsThreadSummary {
    thread_id: String,
    coordination_scope: String,
    rank: u8,
    profile_id: String,
    cwd: String,
    current_task_id: Option<String>,
    current_command_id: Option<String>,
    state: String,
    priority: i32,
    heartbeat_at: String,
}

impl From<ThreadRegistryEntry> for AgentOsThreadSummary {
    fn from(entry: ThreadRegistryEntry) -> Self {
        Self {
            thread_id: entry.thread_id.to_string(),
            coordination_scope: entry.coordination_scope,
            rank: entry.rank,
            profile_id: entry.profile_id,
            cwd: entry.cwd.display().to_string(),
            current_task_id: entry.current_task_id,
            current_command_id: entry.current_command_id,
            state: format!("{:?}", entry.state),
            priority: entry.priority,
            heartbeat_at: entry.heartbeat_at.to_rfc3339(),
        }
    }
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
    created_at: String,
}

impl From<ArtifactRecord> for AgentOsArtifactSummary {
    fn from(artifact: ArtifactRecord) -> Self {
        let mut summary = artifact.summary;
        if summary.len() > 500 {
            summary.truncate(500);
        }
        Self {
            artifact_id: artifact.artifact_id,
            task_id: artifact.task_id,
            owner_thread_id: artifact.owner_thread_id.to_string(),
            artifact_type: format!("{:?}", artifact.artifact_type),
            uri: artifact.uri,
            summary,
            created_at: artifact.created_at.to_rfc3339(),
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

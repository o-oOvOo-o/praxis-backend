use chrono::{DateTime, Utc};
use praxis_protocol::ThreadId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ThreadRuntimeState {
    Idle,
    Assigned,
    Running,
    WaitingForLease,
    WaitingForCoordinator,
    Stopping,
    Stopped,
    Failed,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum ActionIntentKind {
    ReadOnly,
    FileWrite,
    Harness,
    Test,
    Compile,
    RunApp,
    LongProcess,
    Network,
    Gpu,
    GitMutation,
    UnknownRisky,
}

impl ActionIntentKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::FileWrite => "file_write",
            Self::Harness => "harness",
            Self::Test => "test",
            Self::Compile => "compile",
            Self::RunApp => "run_app",
            Self::LongProcess => "long_process",
            Self::Network => "network",
            Self::Gpu => "gpu",
            Self::GitMutation => "git_mutation",
            Self::UnknownRisky => "unknown_risky",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum ResourceRequirement {
    CpuHeavy,
    BuildCache { scope: String },
    AppRuntime { scope: String },
    Port { port: u16 },
    RepoWrite { scope: String },
    LlmBudget { scope: String },
    Gpu { scope: String },
    Network { scope: String },
    GitIndex { scope: String },
}

impl ResourceRequirement {
    pub(crate) fn key(&self) -> String {
        match self {
            Self::CpuHeavy => "cpu_heavy:global".to_string(),
            Self::BuildCache { scope } => format!("build_cache:{scope}"),
            Self::AppRuntime { scope } => format!("app_runtime:{scope}"),
            Self::Port { port } => format!("port:{port}"),
            Self::RepoWrite { scope } => format!("repo_write:{scope}"),
            Self::LlmBudget { scope } => format!("llm_budget:{scope}"),
            Self::Gpu { scope } => format!("gpu:{scope}"),
            Self::Network { scope } => format!("network:{scope}"),
            Self::GitIndex { scope } => format!("git_index:{scope}"),
        }
    }

    pub(crate) fn parse_spec(resource: &str) -> Result<Self, String> {
        resource.parse()
    }

    pub(super) fn resource_type(&self) -> &'static str {
        match self {
            Self::CpuHeavy => "cpu_heavy",
            Self::BuildCache { .. } => "build_cache",
            Self::AppRuntime { .. } => "app_runtime",
            Self::Port { .. } => "port",
            Self::RepoWrite { .. } => "repo_write",
            Self::LlmBudget { .. } => "llm_budget",
            Self::Gpu { .. } => "gpu",
            Self::Network { .. } => "network",
            Self::GitIndex { .. } => "git_index",
        }
    }

    pub(super) fn mode(&self) -> LeaseMode {
        match self {
            Self::CpuHeavy | Self::LlmBudget { .. } => LeaseMode::Capacity,
            _ => LeaseMode::Exclusive,
        }
    }
}

impl fmt::Display for ResourceRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CpuHeavy => f.write_str("cpu_heavy"),
            Self::BuildCache { scope } => write!(f, "build_cache:{scope}"),
            Self::AppRuntime { scope } => write!(f, "app_runtime:{scope}"),
            Self::Port { port } => write!(f, "port:{port}"),
            Self::RepoWrite { scope } => write!(f, "repo_write:{scope}"),
            Self::LlmBudget { scope } => write!(f, "llm_budget:{scope}"),
            Self::Gpu { scope } => write!(f, "gpu:{scope}"),
            Self::Network { scope } => write!(f, "network:{scope}"),
            Self::GitIndex { scope } => write!(f, "git_index:{scope}"),
        }
    }
}

impl FromStr for ResourceRequirement {
    type Err = String;

    fn from_str(resource: &str) -> Result<Self, Self::Err> {
        let resource = resource.trim();
        if resource.is_empty() {
            return Err("resource requirement cannot be empty".to_string());
        }
        let (kind, scope) = resource
            .split_once(':')
            .map(|(kind, scope)| (kind.trim(), Some(scope.trim())))
            .unwrap_or((resource, None));
        match kind {
            "cpu_heavy" => Ok(Self::CpuHeavy),
            "build_cache" => Ok(Self::BuildCache {
                scope: required_resource_scope(resource, scope)?,
            }),
            "app_runtime" => Ok(Self::AppRuntime {
                scope: required_resource_scope(resource, scope)?,
            }),
            "port" => {
                let port = required_resource_scope(resource, scope)?
                    .parse::<u16>()
                    .map_err(|_| format!("port resource must use a u16 port: `{resource}`"))?;
                Ok(Self::Port { port })
            }
            "repo_write" | "file_write" => Ok(Self::RepoWrite {
                scope: required_resource_scope(resource, scope)?,
            }),
            "llm_budget" => Ok(Self::LlmBudget {
                scope: optional_resource_scope(scope, "task"),
            }),
            "gpu" => Ok(Self::Gpu {
                scope: optional_resource_scope(scope, "default"),
            }),
            "network" => Ok(Self::Network {
                scope: optional_resource_scope(scope, "default"),
            }),
            "git_index" => Ok(Self::GitIndex {
                scope: required_resource_scope(resource, scope)?,
            }),
            _ => Err(format!("unknown resource requirement `{resource}`")),
        }
    }
}

fn required_resource_scope(resource: &str, scope: Option<&str>) -> Result<String, String> {
    let Some(scope) = scope.filter(|scope| !scope.is_empty()) else {
        return Err(format!(
            "resource requirement `{resource}` requires a scope"
        ));
    };
    Ok(scope.to_string())
}

fn optional_resource_scope(scope: Option<&str>, default_scope: &str) -> String {
    scope
        .filter(|scope| !scope.is_empty())
        .unwrap_or(default_scope)
        .to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LeaseMode {
    Exclusive,
    Shared,
    Capacity,
    Advisory,
}

impl LeaseMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Exclusive => "exclusive",
            Self::Shared => "shared",
            Self::Capacity => "capacity",
            Self::Advisory => "advisory",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ActionIntent {
    pub(crate) kind: ActionIntentKind,
    pub(crate) confidence: f32,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) side_effects: Vec<String>,
    pub(crate) risk_level: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ThreadRegistryEntry {
    pub(crate) thread_id: ThreadId,
    pub(crate) coordination_scope: String,
    pub(crate) rank: u8,
    pub(crate) profile_id: String,
    pub(crate) cwd: PathBuf,
    pub(crate) repo_id: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<PathBuf>,
    pub(crate) current_task_id: Option<String>,
    pub(crate) current_command_id: Option<String>,
    pub(crate) state: ThreadRuntimeState,
    pub(crate) heartbeat_at: DateTime<Utc>,
    pub(crate) priority: i32,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(crate) struct ThreadRegistration {
    pub(crate) thread_id: ThreadId,
    pub(crate) coordination_scope: String,
    pub(crate) rank: u8,
    pub(crate) profile_id: String,
    pub(crate) cwd: PathBuf,
    pub(crate) repo_id: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<PathBuf>,
    pub(crate) priority: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CapabilityProfile {
    pub(crate) profile_id: String,
    pub(crate) can_read_files: bool,
    pub(crate) can_write_files: bool,
    pub(crate) can_run_shell: bool,
    pub(crate) can_cpu_heavy: bool,
    pub(crate) can_compile: bool,
    pub(crate) can_run_app: bool,
    pub(crate) can_use_gpu: bool,
    pub(crate) can_hold_ports: bool,
    pub(crate) can_network: bool,
    pub(crate) can_modify_git: bool,
    pub(crate) can_spawn_long_process: bool,
    pub(crate) path_scopes: ScopedPaths,
    pub(crate) intent_scopes: ScopedIntents,
    pub(crate) command_denylist: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ScopedPaths {
    pub(crate) allow: Vec<String>,
    pub(crate) deny: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ScopedIntents {
    pub(crate) allow: Vec<ActionIntentKind>,
    pub(crate) deny: Vec<ActionIntentKind>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TaskRecord {
    pub(crate) task_id: String,
    pub(crate) objective: String,
    pub(crate) scope: Vec<String>,
    pub(crate) constraints: Vec<String>,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) artifact_refs: Vec<String>,
    pub(crate) status: TaskStatus,
    pub(crate) priority: i32,
    pub(crate) assigned_thread_id: Option<ThreadId>,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) token_budget: Option<u64>,
    #[serde(default)]
    pub(crate) artifact_read_bytes: u64,
    pub(crate) exploratory: bool,
    pub(crate) created_by: ThreadId,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TaskStatus {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub(crate) struct TaskCreateRequest {
    pub(crate) objective: String,
    pub(crate) scope: Vec<String>,
    pub(crate) constraints: Vec<String>,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) artifact_refs: Vec<String>,
    pub(crate) priority: i32,
    pub(crate) assigned_thread_id: Option<ThreadId>,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) token_budget: Option<u64>,
    pub(crate) exploratory: bool,
    pub(crate) created_by: ThreadId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ResourceLease {
    pub(crate) lease_id: String,
    pub(crate) resource_type: String,
    pub(crate) scope: String,
    pub(crate) mode: LeaseMode,
    pub(crate) owner_thread_id: ThreadId,
    pub(crate) task_id: String,
    pub(crate) priority: i32,
    pub(crate) fencing_token: u64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) expires_at: Option<DateTime<Utc>>,
    pub(crate) revocable: bool,
    pub(crate) metadata: serde_json::Value,
    pub(crate) command_id: Option<String>,
    pub(crate) process_id: Option<i32>,
    pub(crate) runtime_owner_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ExecutionTicket {
    pub(crate) ticket_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) coordination_scope: String,
    pub(crate) allowed_intent: ActionIntentKind,
    pub(crate) intent_plan_id: Option<String>,
    pub(crate) command_fingerprint: String,
    pub(crate) cwd: PathBuf,
    pub(crate) risk_level: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) lease_ids: Vec<String>,
    pub(crate) file_scopes: Vec<String>,
    pub(crate) token_budget: Option<u64>,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) fencing_token: u64,
    pub(crate) coordinator_epoch: u64,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CommandIntentPlan {
    pub(crate) plan_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) intent: ActionIntentKind,
    pub(crate) confidence: f32,
    pub(crate) command_fingerprint: String,
    pub(crate) command: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) side_effects: Vec<String>,
    pub(crate) risk_level: String,
    pub(crate) status: CommandIntentPlanStatus,
    pub(crate) consumed_by_ticket_id: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CommandIntentPlanStatus {
    Pending,
    Consumed,
    Expired,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ManagedProcessStatus {
    Running,
    Cleaning,
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ManagedProcessRecord {
    pub(crate) process_id: i32,
    pub(crate) command_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) cwd: PathBuf,
    pub(crate) runtime_kind: String,
    pub(crate) runtime_owner_id: Option<String>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) last_heartbeat: DateTime<Utc>,
    pub(crate) ended_at: Option<DateTime<Utc>>,
    pub(crate) status: ManagedProcessStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CommandRecord {
    pub(crate) command_id: String,
    pub(crate) ticket_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) intent: ActionIntentKind,
    pub(crate) intent_plan_id: Option<String>,
    pub(crate) command_fingerprint: String,
    pub(crate) raw_command: String,
    pub(crate) cwd: PathBuf,
    pub(crate) process_id: Option<i32>,
    pub(crate) runtime_kind: Option<String>,
    pub(crate) runtime_owner_id: Option<String>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) ended_at: Option<DateTime<Utc>>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) lease_ids: Vec<String>,
    pub(crate) artifacts: Vec<String>,
    pub(crate) baseline_dirty_files: Vec<PathBuf>,
    pub(super) baseline_dirty_fingerprints: HashMap<String, DirtyFileFingerprint>,
    pub(crate) dirty_files: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct DirtyFileFingerprint {
    pub(super) exists: bool,
    pub(super) len: Option<u64>,
    pub(super) modified_unix_millis: Option<i128>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ArtifactRecord {
    pub(crate) artifact_id: String,
    pub(crate) task_id: String,
    pub(crate) owner_thread_id: ThreadId,
    pub(crate) artifact_type: ArtifactType,
    pub(crate) uri: String,
    pub(crate) summary: String,
    pub(crate) metadata: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ArtifactBlobRead {
    pub(crate) artifact: ArtifactRecord,
    pub(crate) content: String,
    pub(crate) bytes_read: usize,
    pub(crate) blob_bytes: Option<u64>,
    pub(crate) truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ArtifactType {
    CommandLog,
    CompileLog,
    DirtyFileReport,
    DecisionRecord,
    PatchMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WorkerRequestStatus {
    Pending,
    Approved,
    Rejected,
    Resolved,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WorkerRequestRecord {
    pub(crate) request_id: String,
    pub(crate) request_type: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) task_id: Option<String>,
    pub(crate) blocking: bool,
    pub(crate) status: WorkerRequestStatus,
    pub(crate) reason: String,
    pub(crate) requested_resource: Option<String>,
    pub(crate) artifact_refs: Vec<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkerRequestCreateRequest {
    pub(crate) request_type: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) blocking: bool,
    pub(crate) reason: String,
    pub(crate) requested_resource: Option<String>,
    pub(crate) artifact_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RuntimeCommandType {
    AssignTask,
    Pause,
    Resume,
    YieldLease,
    CancelCommand,
    Terminate,
    StatusQuery,
    SetPriority,
    GrantTemporaryCapability,
    RevokeTemporaryCapability,
    RequestArtifact,
    RequestSummary,
}

impl RuntimeCommandType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AssignTask => "assign_task",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::YieldLease => "yield_lease",
            Self::CancelCommand => "cancel_command",
            Self::Terminate => "terminate",
            Self::StatusQuery => "status_query",
            Self::SetPriority => "set_priority",
            Self::GrantTemporaryCapability => "grant_temporary_capability",
            Self::RevokeTemporaryCapability => "revoke_temporary_capability",
            Self::RequestArtifact => "request_artifact",
            Self::RequestSummary => "request_summary",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RuntimeCommandStatus {
    Pending,
    Acked,
    Executing,
    Completed,
    Failed,
    Expired,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuntimeCommandActivity {
    WorkerHeartbeat,
    WorkerStartedCommand,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RuntimeCommandRecord {
    pub(crate) command_id: String,
    pub(crate) from_thread_id: ThreadId,
    pub(crate) to_thread_id: ThreadId,
    pub(crate) task_id: Option<String>,
    pub(crate) coordinator_epoch: u64,
    pub(crate) fencing_token: u64,
    pub(crate) command_type: RuntimeCommandType,
    pub(crate) payload: serde_json::Value,
    pub(crate) status: RuntimeCommandStatus,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct EventLedgerEntry {
    pub(crate) event_id: String,
    pub(crate) event_type: String,
    pub(crate) thread_id: Option<ThreadId>,
    pub(crate) task_id: Option<String>,
    pub(crate) command_id: Option<String>,
    pub(crate) payload: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ActiveCoordinatorLease {
    pub(super) coordination_scope: String,
    pub(super) owner_thread_id: ThreadId,
    pub(super) epoch: u64,
    pub(super) fencing_token: u64,
    pub(super) expires_at: DateTime<Utc>,
}

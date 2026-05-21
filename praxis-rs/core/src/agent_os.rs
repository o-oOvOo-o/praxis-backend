use async_trait::async_trait;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::sync::watch;
use uuid::Uuid;

use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::exec::ExecOutputSpool;
use crate::exec::ExecStreamSpool;
use crate::path_scope::normalize_path_for_scope;
use crate::path_scope::scope_matches;
use crate::path_scope::wildcard_match;
use crate::util::truncate_to_char_boundary;
use praxis_rollout::StateDbHandle;

const COORDINATOR_RANK: u8 = 0;
const MAX_COORDINATORS: usize = 3;
const DEFAULT_TICKET_TTL_SECONDS: i64 = 30 * 60;
const DEFAULT_LEASE_TTL_SECONDS: i64 = 30 * 60;
const LEASE_JANITOR_INTERVAL_SECONDS: u64 = 30;
const MAX_AGENT_OS_EVENTS_IN_MEMORY: usize = 1_000;
const DEFAULT_ARTIFACT_READ_MAX_BYTES: usize = 64 * 1024;
const HARD_ARTIFACT_READ_MAX_BYTES: usize = 1024 * 1024;

static AGENT_OS_POLICY: OnceLock<AgentOsPolicy> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
struct AgentOsPolicy {
    ticket_ttl_seconds: i64,
    lease_ttl_seconds: i64,
    max_events_in_memory: usize,
    default_artifact_read_max_bytes: usize,
}

impl AgentOsPolicy {
    fn get() -> &'static Self {
        AGENT_OS_POLICY.get_or_init(|| Self {
            ticket_ttl_seconds: read_i64_env(
                "PRAXIS_AGENTOS_TICKET_TTL_SECONDS",
                DEFAULT_TICKET_TTL_SECONDS,
                60,
                24 * 60 * 60,
            ),
            lease_ttl_seconds: read_i64_env(
                "PRAXIS_AGENTOS_LEASE_TTL_SECONDS",
                DEFAULT_LEASE_TTL_SECONDS,
                60,
                24 * 60 * 60,
            ),
            max_events_in_memory: read_usize_env(
                "PRAXIS_AGENTOS_MAX_EVENTS_IN_MEMORY",
                MAX_AGENT_OS_EVENTS_IN_MEMORY,
                1,
                100_000,
            ),
            default_artifact_read_max_bytes: read_usize_env(
                "PRAXIS_AGENTOS_ARTIFACT_READ_MAX_BYTES",
                DEFAULT_ARTIFACT_READ_MAX_BYTES,
                1,
                HARD_ARTIFACT_READ_MAX_BYTES,
            ),
        })
    }

    fn ticket_ttl(&self) -> Duration {
        Duration::seconds(self.ticket_ttl_seconds)
    }

    fn lease_ttl(&self) -> Duration {
        Duration::seconds(self.lease_ttl_seconds)
    }
}

fn read_i64_env(name: &str, default_value: i64, hard_min: i64, hard_max: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .map(|value| value.clamp(hard_min, hard_max))
        .unwrap_or(default_value)
}

fn read_usize_env(name: &str, default_value: usize, hard_min: usize, hard_max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(hard_min, hard_max))
        .unwrap_or(default_value)
}

pub(crate) mod process_runtime_kind {
    pub(crate) const GENERIC: &str = "generic";
    pub(crate) const SHELL: &str = "shell";
    pub(crate) const ZSH_FORK: &str = "zsh_fork";
    pub(crate) const UNIFIED_EXEC: &str = "unified_exec";
    pub(crate) const LONG_PROCESS: &str = "long_process";
    pub(crate) const GPU_COMMAND: &str = "gpu_command";
    pub(crate) const NETWORK_COMMAND: &str = "network_command";
    pub(crate) const COMMAND: &str = "command";
    pub(crate) const APPLY_PATCH: &str = "apply_patch";
}

pub(crate) mod process_runtime_owner {
    pub(crate) const SHELL: &str = "shell-host";
    pub(crate) const ZSH_FORK: &str = "zsh-fork-host";
}

#[async_trait]
pub(crate) trait AgentOsProcessCleaner: Send + Sync {
    fn runtime_kind(&self) -> &'static str {
        process_runtime_kind::GENERIC
    }

    /// Stable backend identifier for the concrete runtime instance that owns
    /// process ids. Process ids are scoped to runtime backends; using only the
    /// numeric id is unsafe when multiple sessions/managers are live.
    fn runtime_owner_id(&self) -> String {
        self.runtime_kind().to_string()
    }

    async fn cleanup_agent_os_process(&self, process_id: i32) -> bool;
}

fn process_registry_key(process_id: i32, runtime_owner_id: Option<&str>) -> String {
    match runtime_owner_id.filter(|owner| !owner.is_empty()) {
        Some(owner) => format!("{owner}:{process_id}"),
        None => format!("legacy:{process_id}"),
    }
}

fn cleaner_registry_key(runtime_kind: &str, runtime_owner_id: &str) -> String {
    format!("{runtime_kind}:{runtime_owner_id}")
}

fn has_active_assign_runtime_command_locked(
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
    fn as_str(self) -> &'static str {
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

    fn resource_type(&self) -> &'static str {
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

    fn mode(&self) -> LeaseMode {
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
    fn as_str(self) -> &'static str {
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
    baseline_dirty_fingerprints: HashMap<String, DirtyFileFingerprint>,
    pub(crate) dirty_files: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DirtyFileFingerprint {
    exists: bool,
    len: Option<u64>,
    modified_unix_millis: Option<i128>,
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
enum RuntimeCommandActivity {
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
struct ActiveCoordinatorLease {
    coordination_scope: String,
    owner_thread_id: ThreadId,
    epoch: u64,
    fencing_token: u64,
    expires_at: DateTime<Utc>,
}

#[derive(Default)]
struct AgentOsState {
    threads: HashMap<ThreadId, ThreadRegistryEntry>,
    profiles: HashMap<String, CapabilityProfile>,
    tasks: HashMap<String, TaskRecord>,
    leases: HashMap<String, ResourceLease>,
    tickets: HashMap<String, ExecutionTicket>,
    intent_plans: HashMap<String, CommandIntentPlan>,
    commands: HashMap<String, CommandRecord>,
    processes: HashMap<String, ManagedProcessRecord>,
    artifacts: HashMap<String, ArtifactRecord>,
    worker_requests: HashMap<String, WorkerRequestRecord>,
    runtime_commands: HashMap<String, RuntimeCommandRecord>,
    events: Vec<EventLedgerEntry>,
    active_coordinators: HashMap<String, ActiveCoordinatorLease>,
    fencing_counter: u64,
    coordinator_epoch: u64,
}

pub(crate) struct AgentOsRuntime {
    state: RwLock<AgentOsState>,
    state_db: RwLock<Option<StateDbHandle>>,
    // Multiple sessions share one AgentOS runtime. Cleaners are indexed by runtime
    // kind so lease expiry can route process cleanup to the backend that owns the
    // process instead of guessing through every session-level manager.
    process_cleaners: RwLock<HashMap<String, Vec<Arc<dyn AgentOsProcessCleaner>>>>,
    process_cleaners_by_owner: RwLock<HashMap<String, Arc<dyn AgentOsProcessCleaner>>>,
    lease_janitor_started: AtomicBool,
    change_seq: AtomicU64,
    change_tx: watch::Sender<u64>,
}

impl Default for AgentOsRuntime {
    fn default() -> Self {
        let (change_tx, _) = watch::channel(0);
        Self {
            state: RwLock::new(AgentOsState::default()),
            state_db: RwLock::new(None),
            process_cleaners: RwLock::new(HashMap::new()),
            process_cleaners_by_owner: RwLock::new(HashMap::new()),
            lease_janitor_started: AtomicBool::new(false),
            change_seq: AtomicU64::new(0),
            change_tx,
        }
    }
}

#[derive(Clone)]
pub(crate) struct ManagedCommandSpan {
    agent_os: Arc<AgentOsRuntime>,
    command_id: String,
}

struct DirtyAuditOutcome {
    command: CommandRecord,
    thread_snapshot: Option<ThreadRegistryEntry>,
    task_snapshot: Option<TaskRecord>,
    dirty_files: Vec<PathBuf>,
    violation_path: Option<PathBuf>,
}

enum ManagedCommandOutputSource<'a> {
    Bytes(&'a [u8]),
    Spool {
        spool: ExecOutputSpool,
        fallback_raw_output: &'a [u8],
    },
}

impl ManagedCommandOutputSource<'_> {
    fn is_empty(&self) -> bool {
        match self {
            Self::Bytes(bytes) => bytes.is_empty(),
            Self::Spool { spool, .. } => spool.is_empty(),
        }
    }

    fn byte_len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Spool { spool, .. } => spool.total_bytes(),
        }
    }

    fn summary(&self) -> String {
        match self {
            Self::Bytes(bytes) => summarize_output(bytes),
            Self::Spool {
                fallback_raw_output,
                ..
            } => summarize_output(fallback_raw_output),
        }
    }
}

impl ManagedCommandSpan {
    pub(crate) async fn finish_success(&self, raw_output: &[u8]) -> PraxisResult<Option<String>> {
        self.finish(Some(0), raw_output).await
    }

    pub(crate) async fn finish_failure(&self, raw_output: &[u8]) -> PraxisResult<Option<String>> {
        self.finish(Some(-1), raw_output).await
    }

    pub(crate) async fn finish(
        &self,
        exit_code: Option<i32>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        self.agent_os
            .finish_managed_command(self.command_id.as_str(), exit_code, raw_output, true)
            .await
    }

    pub(crate) async fn finish_with_spooled_output(
        &self,
        exit_code: Option<i32>,
        output_spool: ExecOutputSpool,
        fallback_raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        self.agent_os
            .finish_managed_command_with_spooled_output(
                self.command_id.as_str(),
                exit_code,
                output_spool,
                fallback_raw_output,
                true,
            )
            .await
    }

    pub(crate) async fn checkpoint(&self, raw_output: &[u8]) -> PraxisResult<Option<String>> {
        self.agent_os
            .checkpoint_managed_command(self.command_id.as_str(), raw_output)
            .await
    }

    pub(crate) async fn record_dirty_files(&self, dirty_files: Vec<PathBuf>) -> PraxisResult<()> {
        self.agent_os
            .record_command_dirty_files(self.command_id.as_str(), dirty_files)
            .await
    }

    pub(crate) async fn attach_process(&self, process_id: i32) -> PraxisResult<()> {
        self.agent_os
            .attach_process_to_managed_command(self.command_id.as_str(), process_id)
            .await
    }

    pub(crate) async fn raw_command(&self) -> Option<String> {
        self.agent_os
            .command_raw_command(self.command_id.as_str())
            .await
    }
}

impl AgentOsRuntime {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.change_tx.subscribe()
    }

    pub(crate) fn change_sequence(&self) -> u64 {
        self.change_seq.load(Ordering::SeqCst)
    }

    fn notify_changed(&self) {
        let seq = self.change_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.change_tx.send_replace(seq);
    }

    pub(crate) async fn attach_state_db(&self, state_db: Option<StateDbHandle>) {
        if let Some(state_db) = state_db {
            *self.state_db.write().await = Some(state_db);
        }
    }

    pub(crate) async fn attach_process_cleaner<T>(self: &Arc<Self>, process_cleaner: Arc<T>)
    where
        T: AgentOsProcessCleaner + 'static,
    {
        let runtime_kind = process_cleaner.runtime_kind().to_string();
        let runtime_owner_id = process_cleaner.runtime_owner_id();
        let exact_key = cleaner_registry_key(runtime_kind.as_str(), runtime_owner_id.as_str());
        let process_cleaner: Arc<dyn AgentOsProcessCleaner> = process_cleaner;
        self.process_cleaners
            .write()
            .await
            .entry(runtime_kind)
            .or_default()
            .push(Arc::clone(&process_cleaner));
        self.process_cleaners_by_owner
            .write()
            .await
            .insert(exact_key, process_cleaner);
        self.start_lease_janitor();
    }

    fn start_lease_janitor(self: &Arc<Self>) {
        if self
            .lease_janitor_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let runtime = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                LEASE_JANITOR_INTERVAL_SECONDS,
            ));
            loop {
                interval.tick().await;
                let Some(runtime) = runtime.upgrade() else {
                    break;
                };
                runtime.expire_leases().await;
                runtime.expire_intent_plans().await;
                runtime.expire_runtime_commands().await;
                runtime.expire_tickets().await;
            }
        });
    }

    pub(crate) async fn register_thread(
        &self,
        registration: ThreadRegistration,
    ) -> PraxisResult<()> {
        let now = Utc::now();
        let entry = ThreadRegistryEntry {
            thread_id: registration.thread_id,
            coordination_scope: registration.coordination_scope,
            rank: registration.rank,
            profile_id: registration.profile_id,
            cwd: registration.cwd,
            repo_id: registration.repo_id,
            branch: registration.branch,
            worktree: registration.worktree,
            current_task_id: None,
            current_command_id: None,
            state: ThreadRuntimeState::Idle,
            heartbeat_at: now,
            priority: registration.priority,
            created_at: now,
        };

        {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            if entry.rank == COORDINATOR_RANK {
                let coordinator_count = state
                    .threads
                    .values()
                    .filter(|thread| thread.rank == COORDINATOR_RANK)
                    .filter(|thread| thread.coordination_scope == entry.coordination_scope)
                    .filter(|thread| thread.thread_id != entry.thread_id)
                    .filter(|thread| {
                        !matches!(
                            thread.state,
                            ThreadRuntimeState::Stopped
                                | ThreadRuntimeState::Failed
                                | ThreadRuntimeState::Completed
                        )
                    })
                    .count();
                if coordinator_count >= MAX_COORDINATORS {
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "rank-0 coordinator limit reached for scope `{}`",
                        entry.coordination_scope
                    )));
                }
                let active_state = state
                    .active_coordinators
                    .get(entry.coordination_scope.as_str())
                    .map(|active| (active.owner_thread_id, active.expires_at));
                match active_state {
                    Some((owner, expires_at)) if owner == entry.thread_id && expires_at > now => {
                        if let Some(active) = state
                            .active_coordinators
                            .get_mut(entry.coordination_scope.as_str())
                        {
                            active.expires_at = now + AgentOsPolicy::get().lease_ttl();
                        }
                    }
                    Some((_owner, expires_at)) if expires_at > now => {
                        // Another live rank-0 already owns dispatch for this scope.
                        // This coordinator registers as council/advisor, not active scheduler.
                    }
                    _ => {
                        state.coordinator_epoch = state.coordinator_epoch.saturating_add(1);
                        state.fencing_counter = state.fencing_counter.saturating_add(1);
                        let epoch = state.coordinator_epoch;
                        let fencing_token = state.fencing_counter;
                        state.active_coordinators.insert(
                            entry.coordination_scope.clone(),
                            ActiveCoordinatorLease {
                                coordination_scope: entry.coordination_scope.clone(),
                                owner_thread_id: entry.thread_id,
                                epoch,
                                fencing_token,
                                expires_at: now + AgentOsPolicy::get().lease_ttl(),
                            },
                        );
                    }
                }
            }
            state.threads.insert(entry.thread_id, entry.clone());
        }

        self.persist_thread_snapshot(&entry).await;
        self.record_event(
            "thread_registered",
            Some(entry.thread_id),
            None,
            None,
            json!({
                "coordination_scope": entry.coordination_scope,
                "rank": entry.rank,
                "profile_id": entry.profile_id,
                "cwd": entry.cwd,
            }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn ensure_bootstrap_task(
        &self,
        thread_id: ThreadId,
        objective: impl Into<String>,
        scope: Vec<String>,
    ) -> PraxisResult<String> {
        if let Some(task_id) = self
            .state
            .read()
            .await
            .threads
            .get(&thread_id)
            .and_then(|thread| thread.current_task_id.clone())
        {
            return Ok(task_id);
        }
        let task = self
            .create_task(TaskCreateRequest {
                objective: objective.into(),
                scope,
                constraints: Vec::new(),
                acceptance_criteria: Vec::new(),
                artifact_refs: Vec::new(),
                priority: 0,
                assigned_thread_id: Some(thread_id),
                required_capabilities: Vec::new(),
                required_resources: Vec::new(),
                token_budget: None,
                exploratory: true,
                created_by: thread_id,
            })
            .await?;
        self.assign_task(task.as_str(), thread_id).await?;
        Ok(task)
    }

    pub(crate) async fn create_task(&self, request: TaskCreateRequest) -> PraxisResult<String> {
        let now = Utc::now();
        let task_id = format!("task-{}", Uuid::new_v4());
        let assigned_thread_id = request.assigned_thread_id;
        if assigned_thread_id.is_some() && request.scope.is_empty() && !request.exploratory {
            return Err(PraxisErr::UnsupportedOperation(
                "assigned AgentOS tasks require non-empty scope unless exploratory=true"
                    .to_string(),
            ));
        }
        let task = TaskRecord {
            task_id: task_id.clone(),
            objective: request.objective,
            scope: request.scope,
            constraints: request.constraints,
            acceptance_criteria: request.acceptance_criteria,
            artifact_refs: request.artifact_refs,
            status: if assigned_thread_id.is_some() {
                TaskStatus::Assigned
            } else {
                TaskStatus::Pending
            },
            priority: request.priority,
            assigned_thread_id: assigned_thread_id.clone(),
            required_capabilities: request.required_capabilities,
            required_resources: request.required_resources,
            token_budget: request.token_budget,
            artifact_read_bytes: 0,
            exploratory: request.exploratory,
            created_by: request.created_by,
            created_at: now,
            updated_at: now,
        };

        {
            let mut state = self.state.write().await;
            state.tasks.insert(task_id.clone(), task.clone());
        }
        self.persist_task_snapshot(&task).await;
        self.record_event(
            "task_created",
            assigned_thread_id,
            Some(task_id.clone()),
            None,
            json!({
                "objective": task.objective,
                "scope": task.scope,
                "constraints": task.constraints,
                "acceptance_criteria": task.acceptance_criteria,
                "artifact_refs": task.artifact_refs,
                "priority": task.priority,
                "exploratory": task.exploratory,
            }),
        )
        .await;
        Ok(task_id)
    }

    pub(crate) async fn assign_task(&self, task_id: &str, thread_id: ThreadId) -> PraxisResult<()> {
        let (thread_snapshot, task_snapshot) = {
            let mut state = self.state.write().await;
            let task = state.tasks.get_mut(task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown task `{task_id}`"))
            })?;
            task.assigned_thread_id = Some(thread_id);
            task.status = TaskStatus::Assigned;
            task.updated_at = Utc::now();
            let task_snapshot = task.clone();
            let thread = state.threads.get_mut(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            thread.current_task_id = Some(task_id.to_string());
            thread.state = ThreadRuntimeState::Assigned;
            thread.heartbeat_at = Utc::now();
            (thread.clone(), task_snapshot)
        };

        self.persist_thread_snapshot(&thread_snapshot).await;
        self.persist_task_snapshot(&task_snapshot).await;
        self.record_event(
            "task_assigned",
            Some(thread_id),
            Some(task_id.to_string()),
            None,
            json!({ "thread_id": thread_id.to_string() }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn heartbeat_thread(&self, thread_id: ThreadId) -> PraxisResult<()> {
        let now = Utc::now();
        let thread_snapshot = {
            let mut state = self.state.write().await;
            let thread = state.threads.get_mut(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            thread.heartbeat_at = now;
            let snapshot = thread.clone();
            if snapshot.rank == COORDINATOR_RANK
                && let Some(active) = state
                    .active_coordinators
                    .get_mut(snapshot.coordination_scope.as_str())
                && active.owner_thread_id == thread_id
            {
                active.expires_at = now + AgentOsPolicy::get().lease_ttl();
            }
            snapshot
        };
        self.persist_thread_snapshot(&thread_snapshot).await;
        self.note_runtime_command_activity(thread_id, RuntimeCommandActivity::WorkerHeartbeat)
            .await;
        Ok(())
    }

    pub(crate) async fn ensure_inter_thread_message_allowed(
        &self,
        from_thread_id: ThreadId,
        to_thread_id: ThreadId,
        require_active_dispatcher: bool,
    ) -> PraxisResult<()> {
        let state = self.state.read().await;
        let sender = state.threads.get(&from_thread_id).ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS sender thread `{from_thread_id}`"
            ))
        })?;
        let _receiver = state.threads.get(&to_thread_id).ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS receiver thread `{to_thread_id}`"
            ))
        })?;
        if sender.rank != COORDINATOR_RANK {
            return Err(PraxisErr::UnsupportedOperation(
                "worker-to-worker natural-language messaging is disabled by AgentOS; submit artifacts, status, or structured requests instead".to_string(),
            ));
        }
        if _receiver.coordination_scope != sender.coordination_scope {
            return Err(PraxisErr::UnsupportedOperation(
                "inter-thread messaging cannot cross coordination scopes".to_string(),
            ));
        }
        if require_active_dispatcher {
            let active = state
                .active_coordinators
                .get(sender.coordination_scope.as_str())
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation("no active coordinator lease".to_string())
                })?;
            if active.expires_at <= Utc::now() {
                return Err(PraxisErr::UnsupportedOperation(
                    "active coordinator lease has expired".to_string(),
                ));
            }
            if active.owner_thread_id != from_thread_id {
                return Err(PraxisErr::UnsupportedOperation(
                    "only the active rank-0 coordinator can dispatch tasks".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) async fn query_leases(&self) -> Vec<ResourceLease> {
        self.expire_tickets().await;
        self.expire_leases().await;
        self.state.read().await.leases.values().cloned().collect()
    }

    pub(crate) async fn query_artifacts(&self) -> Vec<ArtifactRecord> {
        self.state
            .read()
            .await
            .artifacts
            .values()
            .cloned()
            .collect()
    }

    async fn command_raw_command(&self, command_id: &str) -> Option<String> {
        self.state
            .read()
            .await
            .commands
            .get(command_id)
            .map(|command| command.raw_command.clone())
    }

    pub(crate) async fn query_worker_requests(&self) -> Vec<WorkerRequestRecord> {
        self.state
            .read()
            .await
            .worker_requests
            .values()
            .cloned()
            .collect()
    }

    pub(crate) async fn query_runtime_commands(&self) -> Vec<RuntimeCommandRecord> {
        self.expire_runtime_commands().await;
        self.state
            .read()
            .await
            .runtime_commands
            .values()
            .cloned()
            .collect()
    }

    pub(crate) async fn query_intent_plans(&self) -> Vec<CommandIntentPlan> {
        self.expire_intent_plans().await;
        self.state
            .read()
            .await
            .intent_plans
            .values()
            .cloned()
            .collect()
    }

    async fn note_runtime_command_activity(
        &self,
        thread_id: ThreadId,
        activity: RuntimeCommandActivity,
    ) -> Vec<RuntimeCommandRecord> {
        let now = Utc::now();
        let changed = {
            let mut state = self.state.write().await;
            let current_task_id = state
                .threads
                .get_mut(&thread_id)
                .map(|thread| {
                    thread.heartbeat_at = now;
                    thread.current_task_id.clone()
                })
                .unwrap_or_default();
            let mut changed = Vec::new();
            for command in state.runtime_commands.values_mut() {
                if command.to_thread_id != thread_id {
                    continue;
                }
                if command.expires_at <= now {
                    if matches!(
                        command.status,
                        RuntimeCommandStatus::Pending
                            | RuntimeCommandStatus::Acked
                            | RuntimeCommandStatus::Executing
                    ) {
                        command.status = RuntimeCommandStatus::Expired;
                        command.updated_at = now;
                        changed.push(command.clone());
                    }
                    continue;
                }
                let mut command_changed = false;
                if matches!(
                    command.status,
                    RuntimeCommandStatus::Pending
                        | RuntimeCommandStatus::Acked
                        | RuntimeCommandStatus::Executing
                ) {
                    command.expires_at = now + AgentOsPolicy::get().ticket_ttl();
                    command.updated_at = now;
                    command_changed = true;
                }
                match (activity, command.status, command.command_type) {
                    (_, RuntimeCommandStatus::Pending, _) => {
                        command.status = RuntimeCommandStatus::Acked;
                        command_changed = true;
                    }
                    (
                        RuntimeCommandActivity::WorkerStartedCommand,
                        RuntimeCommandStatus::Acked,
                        RuntimeCommandType::AssignTask,
                    ) if command.task_id == current_task_id => {
                        command.status = RuntimeCommandStatus::Executing;
                        command_changed = true;
                    }
                    _ => {}
                }
                if command_changed {
                    changed.push(command.clone());
                }
            }
            changed
        };
        for command in &changed {
            self.persist_runtime_command_snapshot(command).await;
        }
        if !changed.is_empty() {
            self.record_event(
                "runtime_command_activity_synced",
                Some(thread_id),
                None,
                None,
                json!({
                    "activity": format!("{:?}", activity),
                    "changed_commands": changed
                        .iter()
                        .map(|command| json!({
                            "command_id": &command.command_id,
                            "command_type": command.command_type.as_str(),
                            "status": format!("{:?}", command.status),
                            "task_id": &command.task_id,
                        }))
                        .collect::<Vec<_>>(),
                }),
            )
            .await;
        }
        changed
    }

    pub(crate) async fn complete_active_runtime_command_for_thread(
        &self,
        thread_id: ThreadId,
        succeeded: bool,
        reason: impl Into<String>,
    ) -> PraxisResult<Option<RuntimeCommandRecord>> {
        let reason = reason.into();
        let (command_id, task_id, blocked, task_already_failed) = {
            let state = self.state.read().await;
            let candidate = state
                .runtime_commands
                .values()
                .filter(|command| {
                    command.to_thread_id == thread_id
                        && command.command_type == RuntimeCommandType::AssignTask
                        && matches!(
                            command.status,
                            RuntimeCommandStatus::Pending
                                | RuntimeCommandStatus::Acked
                                | RuntimeCommandStatus::Executing
                        )
                })
                .max_by_key(|command| {
                    let status_rank = match command.status {
                        RuntimeCommandStatus::Executing => 2,
                        RuntimeCommandStatus::Acked => 1,
                        RuntimeCommandStatus::Pending => 0,
                        RuntimeCommandStatus::Completed
                        | RuntimeCommandStatus::Failed
                        | RuntimeCommandStatus::Expired
                        | RuntimeCommandStatus::Rejected => -1,
                    };
                    (status_rank, command.updated_at.timestamp_millis())
                })
                .cloned();
            let Some(command) = candidate else {
                return Ok(None);
            };
            let blocked = command.task_id.as_ref().is_some_and(|task_id| {
                state.worker_requests.values().any(|request| {
                    request.thread_id == thread_id
                        && request.task_id.as_deref() == Some(task_id.as_str())
                        && request.blocking
                        && matches!(request.status, WorkerRequestStatus::Pending)
                })
            });
            let task_already_failed = command.task_id.as_ref().is_some_and(|task_id| {
                state.tasks.get(task_id).is_some_and(|task| {
                    matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled)
                })
            });
            (
                command.command_id,
                command.task_id.clone(),
                blocked,
                task_already_failed,
            )
        };

        if succeeded && !task_already_failed && blocked {
            self.record_event(
                "runtime_command_completion_deferred",
                Some(thread_id),
                task_id,
                Some(command_id),
                json!({
                    "reason": reason,
                    "deferred_because": "pending_blocking_worker_request",
                }),
            )
            .await;
            return Ok(None);
        }

        let status = if succeeded && !task_already_failed {
            RuntimeCommandStatus::Completed
        } else {
            RuntimeCommandStatus::Failed
        };
        let command = self
            .update_runtime_command_status(command_id.as_str(), thread_id, status)
            .await?;
        self.record_event(
            "runtime_command_lifecycle_completed",
            Some(thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "status": format!("{:?}", command.status),
                "reason": reason,
                "task_already_failed": task_already_failed,
            }),
        )
        .await;
        Ok(Some(command))
    }

    pub(crate) async fn preflight_command_intent(
        &self,
        thread_id: ThreadId,
        command: &[String],
        cwd: &Path,
    ) -> PraxisResult<CommandIntentPlan> {
        self.note_runtime_command_activity(thread_id, RuntimeCommandActivity::WorkerStartedCommand)
            .await;
        let intent = classify_command(command, cwd);
        let now = Utc::now();
        let (thread, task, profile) = {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "AgentOS thread `{thread_id}` is not registered"
                ))
            })?;
            let task_id = thread.current_task_id.clone().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(
                    "side-effectful action rejected: thread has no current task_id".to_string(),
                )
            })?;
            let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "current task `{task_id}` is not registered"
                ))
            })?;
            let profile = state
                .profiles
                .get(&thread.profile_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown capability profile `{}`",
                        thread.profile_id
                    ))
                })?;
            (thread, task, profile)
        };

        profile
            .validate_command_intent(&intent, command, cwd)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let plan = CommandIntentPlan {
            plan_id: format!("intent-plan-{}", Uuid::new_v4()),
            task_id: task.task_id,
            thread_id,
            intent: intent.kind,
            confidence: intent.confidence,
            command_fingerprint: action_fingerprint(command, cwd, intent.kind),
            command: command.to_vec(),
            cwd: cwd.to_path_buf(),
            required_capabilities,
            required_resources: intent.required_resources,
            side_effects: intent.side_effects,
            risk_level: intent.risk_level,
            status: CommandIntentPlanStatus::Pending,
            consumed_by_ticket_id: None,
            created_at: now,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
        };

        self.insert_intent_plan(&plan).await;
        self.persist_intent_plan_snapshot(&plan).await;
        self.record_event(
            "command_intent_preflight",
            Some(thread.thread_id),
            Some(plan.task_id.clone()),
            None,
            json!({
                "plan_id": &plan.plan_id,
                "intent": plan.intent.as_str(),
                "confidence": plan.confidence,
                "risk_level": &plan.risk_level,
                "status": format!("{:?}", plan.status),
                "expires_at": plan.expires_at.to_rfc3339(),
                "required_capabilities": &plan.required_capabilities,
                "required_resources": plan
                    .required_resources
                    .iter()
                    .map(ResourceRequirement::key)
                    .collect::<Vec<_>>(),
                "cwd": &plan.cwd,
            }),
        )
        .await;
        Ok(plan)
    }

    pub(crate) async fn preflight_mutating_tool_intent(
        &self,
        thread_id: ThreadId,
        tool_name: &str,
        arguments_fingerprint_source: &str,
    ) -> PraxisResult<CommandIntentPlan> {
        self.note_runtime_command_activity(thread_id, RuntimeCommandActivity::WorkerStartedCommand)
            .await;
        let intent = classify_mutating_tool(tool_name);
        let now = Utc::now();
        let action = vec![
            format!("tool:{tool_name}"),
            arguments_fingerprint_source.to_string(),
        ];
        let (thread, task, profile) = {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "AgentOS thread `{thread_id}` is not registered"
                ))
            })?;
            let task_id = thread.current_task_id.clone().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(
                    "side-effectful tool rejected: thread has no current task_id".to_string(),
                )
            })?;
            let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "current task `{task_id}` is not registered"
                ))
            })?;
            let profile = state
                .profiles
                .get(&thread.profile_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown capability profile `{}`",
                        thread.profile_id
                    ))
                })?;
            (thread, task, profile)
        };

        profile
            .validate_tool_intent(&intent)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let plan = CommandIntentPlan {
            plan_id: format!("intent-plan-{}", Uuid::new_v4()),
            task_id: task.task_id,
            thread_id,
            intent: intent.kind,
            confidence: intent.confidence,
            command_fingerprint: action_fingerprint(&action, thread.cwd.as_path(), intent.kind),
            command: action,
            cwd: thread.cwd,
            required_capabilities,
            required_resources: intent.required_resources,
            side_effects: intent.side_effects,
            risk_level: intent.risk_level,
            status: CommandIntentPlanStatus::Pending,
            consumed_by_ticket_id: None,
            created_at: now,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
        };
        self.insert_intent_plan(&plan).await;
        self.persist_intent_plan_snapshot(&plan).await;
        self.record_event(
            "mutating_tool_intent_preflight",
            Some(thread_id),
            Some(plan.task_id.clone()),
            None,
            json!({
                "plan_id": &plan.plan_id,
                "tool": tool_name,
                "intent": plan.intent.as_str(),
                "confidence": plan.confidence,
                "risk_level": &plan.risk_level,
                "status": format!("{:?}", plan.status),
                "expires_at": plan.expires_at.to_rfc3339(),
                "required_capabilities": &plan.required_capabilities,
                "required_resources": plan
                    .required_resources
                    .iter()
                    .map(ResourceRequirement::key)
                    .collect::<Vec<_>>(),
            }),
        )
        .await;
        Ok(plan)
    }

    /// Return whether a worker has a pending structured command that should
    /// start or feed its next turn. This is intentionally non-mutating so
    /// callers can use it as a wake-up predicate without consuming commands.
    pub(crate) async fn has_claimable_runtime_command_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> bool {
        let now = Utc::now();
        let state = self.state.read().await;
        let Some(thread) = state.threads.get(&thread_id) else {
            return false;
        };
        let Some(active) = state
            .active_coordinators
            .get(thread.coordination_scope.as_str())
        else {
            return false;
        };
        if active.expires_at <= now {
            return false;
        }
        state.runtime_commands.values().any(|command| {
            command.to_thread_id == thread_id
                && matches!(command.status, RuntimeCommandStatus::Pending)
                && command.expires_at > now
                && command.coordinator_epoch == active.epoch
                && command.fencing_token == active.fencing_token
        })
    }

    /// Claim runtime commands for injection into the worker's next turn.
    ///
    /// This is the runtime-lifecycle path, not a model-driven tool call: the
    /// worker does not need to remember to call `poll_runtime_commands` before
    /// receiving its assignment. Claiming a command marks it as consumed by the
    /// runtime. AssignTask commands move directly into Executing so they are
    /// not re-injected on later turns; other command types are Acked and remain
    /// visible to explicit status tools. Already-Acked commands are not claimed
    /// again, which prevents non-AssignTask commands from being injected every
    /// turn forever.
    pub(crate) async fn claim_runtime_commands_for_turn(
        &self,
        thread_id: ThreadId,
    ) -> PraxisResult<Vec<RuntimeCommandRecord>> {
        let now = Utc::now();
        let ttl = AgentOsPolicy::get().ticket_ttl();
        let (claimed, changed_commands, changed_tasks, changed_threads) = {
            let mut state = self.state.write().await;
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            let active = state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned();
            let mut claimed = Vec::new();
            let mut changed_commands = Vec::new();
            let mut changed_tasks = Vec::new();
            let mut changed_threads = Vec::new();
            let command_ids = state
                .runtime_commands
                .iter()
                .filter_map(|(command_id, command)| {
                    if command.to_thread_id != thread_id {
                        return None;
                    }
                    if !matches!(command.status, RuntimeCommandStatus::Pending) {
                        return None;
                    }
                    Some(command_id.clone())
                })
                .collect::<Vec<_>>();

            for command_id in command_ids {
                let Some(command_snapshot) = state.runtime_commands.get(&command_id).cloned()
                else {
                    continue;
                };
                let active_matches = active.as_ref().is_some_and(|active| {
                    active.expires_at > now
                        && command_snapshot.coordinator_epoch == active.epoch
                        && command_snapshot.fencing_token == active.fencing_token
                });
                let next_status = if command_snapshot.expires_at <= now {
                    RuntimeCommandStatus::Expired
                } else if !active_matches {
                    RuntimeCommandStatus::Rejected
                } else if command_snapshot.command_type == RuntimeCommandType::AssignTask {
                    RuntimeCommandStatus::Executing
                } else {
                    RuntimeCommandStatus::Acked
                };

                let Some(command) = state.runtime_commands.get_mut(&command_id) else {
                    continue;
                };
                command.status = next_status;
                command.updated_at = now;
                command.expires_at = now + ttl;
                let updated_command = command.clone();
                changed_commands.push(updated_command.clone());

                if next_status == RuntimeCommandStatus::Executing {
                    if let Some(task_id) = updated_command.task_id.as_deref() {
                        if let Some(task) = state.tasks.get_mut(task_id) {
                            task.status = TaskStatus::Running;
                            task.updated_at = now;
                            changed_tasks.push(task.clone());
                        }
                        if let Some(thread) = state.threads.get_mut(&thread_id) {
                            thread.current_task_id = Some(task_id.to_string());
                            thread.current_command_id = Some(updated_command.command_id.clone());
                            thread.state = ThreadRuntimeState::Running;
                            thread.heartbeat_at = now;
                            changed_threads.push(thread.clone());
                        }
                    }
                }

                if matches!(
                    next_status,
                    RuntimeCommandStatus::Acked | RuntimeCommandStatus::Executing
                ) {
                    claimed.push(updated_command);
                }
            }
            (claimed, changed_commands, changed_tasks, changed_threads)
        };

        for command in &changed_commands {
            self.persist_runtime_command_snapshot(command).await;
        }
        for task in &changed_tasks {
            self.persist_task_snapshot(task).await;
        }
        for thread in &changed_threads {
            self.persist_thread_snapshot(thread).await;
        }
        if !changed_commands.is_empty() {
            self.record_event(
                "runtime_commands_claimed_for_turn",
                Some(thread_id),
                None,
                None,
                json!({
                    "claimed_commands": claimed.iter().map(|command| json!({
                        "command_id": &command.command_id,
                        "command_type": command.command_type.as_str(),
                        "task_id": &command.task_id,
                        "status": format!("{:?}", command.status),
                    })).collect::<Vec<_>>(),
                    "changed_commands": changed_commands.iter().map(|command| json!({
                        "command_id": &command.command_id,
                        "status": format!("{:?}", command.status),
                    })).collect::<Vec<_>>(),
                }),
            )
            .await;
        }

        Ok(claimed)
    }

    pub(crate) async fn poll_runtime_commands(
        &self,
        thread_id: ThreadId,
        auto_ack: bool,
    ) -> PraxisResult<Vec<RuntimeCommandRecord>> {
        let now = Utc::now();
        let (commands, changed_commands) = {
            let mut state = self.state.write().await;
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            let active = state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned();
            let mut commands = Vec::new();
            let mut changed_commands = Vec::new();
            for command in state.runtime_commands.values_mut() {
                if command.to_thread_id != thread_id {
                    continue;
                }
                if !matches!(
                    command.status,
                    RuntimeCommandStatus::Pending
                        | RuntimeCommandStatus::Acked
                        | RuntimeCommandStatus::Executing
                ) {
                    continue;
                }
                if command.expires_at <= now {
                    command.status = RuntimeCommandStatus::Expired;
                    command.updated_at = now;
                    changed_commands.push(command.clone());
                    continue;
                }
                let active_matches = active.as_ref().is_some_and(|active| {
                    active.expires_at > now
                        && command.coordinator_epoch == active.epoch
                        && command.fencing_token == active.fencing_token
                });
                if !active_matches {
                    command.status = RuntimeCommandStatus::Rejected;
                    command.updated_at = now;
                    changed_commands.push(command.clone());
                    continue;
                }
                if auto_ack && command.status == RuntimeCommandStatus::Pending {
                    command.status = RuntimeCommandStatus::Acked;
                    command.updated_at = now;
                    changed_commands.push(command.clone());
                }
                commands.push(command.clone());
            }
            (commands, changed_commands)
        };

        for command in &changed_commands {
            self.persist_runtime_command_snapshot(command).await;
            self.record_event(
                "runtime_command_status_updated",
                Some(thread_id),
                command.task_id.clone(),
                None,
                json!({
                    "command_id": &command.command_id,
                    "from_thread_id": command.from_thread_id.to_string(),
                    "to_thread_id": command.to_thread_id.to_string(),
                    "command_type": command.command_type.as_str(),
                    "status": format!("{:?}", command.status),
                    "source": "poll_runtime_commands",
                }),
            )
            .await;
        }

        Ok(commands)
    }

    pub(crate) async fn issue_runtime_command(
        &self,
        from_thread_id: ThreadId,
        to_thread_id: ThreadId,
        command_type: RuntimeCommandType,
        task_id: Option<String>,
        payload: serde_json::Value,
    ) -> PraxisResult<RuntimeCommandRecord> {
        let now = Utc::now();
        let command_id = format!("runtime-command-{}", Uuid::new_v4());
        let command = {
            let mut state = self.state.write().await;
            let sender = state.threads.get(&from_thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS sender thread `{from_thread_id}`"
                ))
            })?;
            let receiver = state.threads.get(&to_thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS receiver thread `{to_thread_id}`"
                ))
            })?;
            if sender.rank != COORDINATOR_RANK {
                return Err(PraxisErr::UnsupportedOperation(
                    "only rank-0 coordinators can issue runtime commands".to_string(),
                ));
            }
            if sender.coordination_scope != receiver.coordination_scope {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime commands cannot cross coordination scopes".to_string(),
                ));
            }
            if command_type == RuntimeCommandType::AssignTask {
                let Some(task_id) = task_id.as_deref() else {
                    return Err(PraxisErr::UnsupportedOperation(
                        "AssignTask runtime commands require task_id".to_string(),
                    ));
                };
                let task = state.tasks.get(task_id).ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "AssignTask references unknown task `{task_id}`"
                    ))
                })?;
                if task.assigned_thread_id != Some(to_thread_id) {
                    return Err(PraxisErr::UnsupportedOperation(
                        "AssignTask runtime command task owner does not match receiver".to_string(),
                    ));
                }
            }
            let active = state
                .active_coordinators
                .get_mut(sender.coordination_scope.as_str())
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation("no active coordinator lease".to_string())
                })?;
            if active.expires_at <= now {
                return Err(PraxisErr::UnsupportedOperation(
                    "active coordinator lease has expired".to_string(),
                ));
            }
            if active.owner_thread_id != from_thread_id {
                return Err(PraxisErr::UnsupportedOperation(
                    "only the active rank-0 coordinator can issue runtime commands".to_string(),
                ));
            }
            active.expires_at = now + AgentOsPolicy::get().lease_ttl();
            let coordinator_epoch = active.epoch;
            let fencing_token = active.fencing_token;
            let command = RuntimeCommandRecord {
                command_id: command_id.clone(),
                from_thread_id,
                to_thread_id,
                task_id,
                coordinator_epoch,
                fencing_token,
                command_type,
                payload,
                status: RuntimeCommandStatus::Pending,
                created_at: now,
                updated_at: now,
                expires_at: now + AgentOsPolicy::get().ticket_ttl(),
            };
            state.runtime_commands.insert(command_id, command.clone());
            command
        };

        self.persist_runtime_command_snapshot(&command).await;
        self.record_event(
            "runtime_command_issued",
            Some(command.from_thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "to_thread_id": command.to_thread_id.to_string(),
                "command_type": command.command_type.as_str(),
                "status": format!("{:?}", command.status),
                "coordinator_epoch": command.coordinator_epoch,
                "fencing_token": command.fencing_token,
            }),
        )
        .await;
        Ok(command)
    }

    pub(crate) async fn update_runtime_command_status(
        &self,
        command_id: &str,
        actor_thread_id: ThreadId,
        status: RuntimeCommandStatus,
    ) -> PraxisResult<RuntimeCommandRecord> {
        let now = Utc::now();
        let (command, thread_snapshot, task_snapshot) = {
            let mut state = self.state.write().await;
            let existing = state
                .runtime_commands
                .get(command_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown runtime command `{command_id}`"
                    ))
                })?;
            if actor_thread_id != existing.from_thread_id
                && actor_thread_id != existing.to_thread_id
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime command status can only be updated by sender or receiver".to_string(),
                ));
            }
            if matches!(
                status,
                RuntimeCommandStatus::Acked
                    | RuntimeCommandStatus::Executing
                    | RuntimeCommandStatus::Completed
            ) && actor_thread_id != existing.to_thread_id
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime command ack/execution status must be reported by the receiver"
                        .to_string(),
                ));
            }
            let active = state
                .threads
                .get(&existing.to_thread_id)
                .and_then(|thread| {
                    state
                        .active_coordinators
                        .get(thread.coordination_scope.as_str())
                })
                .cloned();
            let active_matches = active.as_ref().is_some_and(|active| {
                active.expires_at > now
                    && existing.coordinator_epoch == active.epoch
                    && existing.fencing_token == active.fencing_token
            });
            let receiver_terminal_report = actor_thread_id == existing.to_thread_id
                && matches!(
                    status,
                    RuntimeCommandStatus::Completed | RuntimeCommandStatus::Failed
                )
                && matches!(
                    existing.status,
                    RuntimeCommandStatus::Acked | RuntimeCommandStatus::Executing
                );
            let status = if receiver_terminal_report
                || matches!(
                    status,
                    RuntimeCommandStatus::Failed | RuntimeCommandStatus::Rejected
                ) {
                status
            } else if existing.expires_at <= now {
                RuntimeCommandStatus::Expired
            } else if !active_matches {
                RuntimeCommandStatus::Rejected
            } else {
                status
            };
            let command = state.runtime_commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown runtime command `{command_id}`"))
            })?;
            command.status = status;
            command.updated_at = now;
            let command_snapshot = command.clone();
            let mut thread_snapshot = None;
            let mut task_snapshot = None;
            if command_snapshot.command_type == RuntimeCommandType::AssignTask
                && let Some(task_id) = command_snapshot.task_id.as_deref()
            {
                if let Some(task) = state.tasks.get_mut(task_id) {
                    task.status = match command_snapshot.status {
                        RuntimeCommandStatus::Executing => TaskStatus::Running,
                        RuntimeCommandStatus::Completed => TaskStatus::Completed,
                        RuntimeCommandStatus::Failed | RuntimeCommandStatus::Expired => {
                            TaskStatus::Failed
                        }
                        RuntimeCommandStatus::Rejected => TaskStatus::Cancelled,
                        RuntimeCommandStatus::Pending | RuntimeCommandStatus::Acked => {
                            TaskStatus::Assigned
                        }
                    };
                    task.updated_at = now;
                    task_snapshot = Some(task.clone());
                }
                if let Some(thread) = state.threads.get_mut(&command_snapshot.to_thread_id) {
                    match command_snapshot.status {
                        RuntimeCommandStatus::Executing => {
                            thread.current_task_id = Some(task_id.to_string());
                            thread.state = ThreadRuntimeState::Running;
                        }
                        RuntimeCommandStatus::Completed
                        | RuntimeCommandStatus::Failed
                        | RuntimeCommandStatus::Rejected
                        | RuntimeCommandStatus::Expired => {
                            if thread.current_task_id.as_deref() == Some(task_id) {
                                thread.current_task_id = None;
                            }
                            thread.state = ThreadRuntimeState::Idle;
                        }
                        RuntimeCommandStatus::Pending | RuntimeCommandStatus::Acked => {
                            thread.current_task_id = Some(task_id.to_string());
                            thread.state = ThreadRuntimeState::Assigned;
                        }
                    }
                    thread.heartbeat_at = now;
                    thread_snapshot = Some(thread.clone());
                }
            }
            (command_snapshot, thread_snapshot, task_snapshot)
        };

        self.persist_runtime_command_snapshot(&command).await;
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if let Some(task) = task_snapshot {
            self.persist_task_snapshot(&task).await;
        }
        self.record_event(
            "runtime_command_status_updated",
            Some(actor_thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "from_thread_id": command.from_thread_id.to_string(),
                "to_thread_id": command.to_thread_id.to_string(),
                "command_type": command.command_type.as_str(),
                "status": format!("{:?}", command.status),
            }),
        )
        .await;
        Ok(command)
    }

    pub(crate) async fn submit_worker_request(
        &self,
        request: WorkerRequestCreateRequest,
    ) -> PraxisResult<WorkerRequestRecord> {
        let request_type = request.request_type.trim().to_string();
        if request_type.is_empty() {
            return Err(PraxisErr::UnsupportedOperation(
                "worker request_type cannot be empty".to_string(),
            ));
        }
        let reason = request.reason.trim().to_string();
        if reason.is_empty() {
            return Err(PraxisErr::UnsupportedOperation(
                "worker request reason cannot be empty".to_string(),
            ));
        }

        let now = Utc::now();
        let request_id = format!("worker-request-{}", Uuid::new_v4());
        let (record, thread_snapshot) = {
            let mut state = self.state.write().await;
            let thread = state.threads.get_mut(&request.thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS thread `{}`",
                    request.thread_id
                ))
            })?;
            let task_id = thread.current_task_id.clone();
            if request.blocking {
                thread.state = if request_type.eq_ignore_ascii_case("BlockedByLease") {
                    ThreadRuntimeState::WaitingForLease
                } else {
                    ThreadRuntimeState::WaitingForCoordinator
                };
                thread.heartbeat_at = now;
            }
            let thread_snapshot = thread.clone();
            let record = WorkerRequestRecord {
                request_id: request_id.clone(),
                request_type,
                thread_id: request.thread_id,
                task_id,
                blocking: request.blocking,
                status: WorkerRequestStatus::Pending,
                reason,
                requested_resource: request.requested_resource,
                artifact_refs: request.artifact_refs,
                created_at: now,
                updated_at: now,
            };
            state.worker_requests.insert(request_id, record.clone());
            (record, thread_snapshot)
        };

        self.persist_thread_snapshot(&thread_snapshot).await;
        self.persist_worker_request_snapshot(&record).await;
        self.record_event(
            "worker_request_submitted",
            Some(record.thread_id),
            record.task_id.clone(),
            None,
            json!({
                "request_id": &record.request_id,
                "request_type": &record.request_type,
                "blocking": record.blocking,
                "status": format!("{:?}", record.status),
                "reason": &record.reason,
                "requested_resource": &record.requested_resource,
                "artifact_refs": &record.artifact_refs,
            }),
        )
        .await;

        Ok(record)
    }

    pub(crate) async fn update_worker_request_status(
        &self,
        request_id: &str,
        actor_thread_id: ThreadId,
        status: WorkerRequestStatus,
    ) -> PraxisResult<WorkerRequestRecord> {
        let now = Utc::now();
        let (record, thread_snapshot) = {
            let mut state = self.state.write().await;
            let existing = state
                .worker_requests
                .get(request_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown worker request `{request_id}`"
                    ))
                })?;
            if actor_thread_id != existing.thread_id {
                let requester =
                    state
                        .threads
                        .get(&existing.thread_id)
                        .cloned()
                        .ok_or_else(|| {
                            PraxisErr::UnsupportedOperation(format!(
                                "unknown AgentOS request thread `{}`",
                                existing.thread_id
                            ))
                        })?;
                let actor = state
                    .threads
                    .get(&actor_thread_id)
                    .cloned()
                    .ok_or_else(|| {
                        PraxisErr::UnsupportedOperation(format!(
                            "unknown AgentOS actor thread `{actor_thread_id}`"
                        ))
                    })?;
                if actor.rank != COORDINATOR_RANK
                    || actor.coordination_scope != requester.coordination_scope
                {
                    return Err(PraxisErr::UnsupportedOperation(
                        "worker request status can only be updated by owner or active coordinator"
                            .to_string(),
                    ));
                }
                let active = state
                    .active_coordinators
                    .get_mut(actor.coordination_scope.as_str())
                    .ok_or_else(|| {
                        PraxisErr::UnsupportedOperation("no active coordinator lease".to_string())
                    })?;
                if active.expires_at <= now || active.owner_thread_id != actor_thread_id {
                    return Err(PraxisErr::UnsupportedOperation(
                        "only the active rank-0 coordinator can resolve worker requests"
                            .to_string(),
                    ));
                }
                active.expires_at = now + AgentOsPolicy::get().lease_ttl();
            }
            let request = state.worker_requests.get_mut(request_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown worker request `{request_id}`"))
            })?;
            request.status = status;
            request.updated_at = now;
            let record = request.clone();
            let thread_snapshot = if record.blocking && status != WorkerRequestStatus::Pending {
                state.threads.get_mut(&record.thread_id).map(|thread| {
                    if matches!(
                        thread.state,
                        ThreadRuntimeState::WaitingForLease
                            | ThreadRuntimeState::WaitingForCoordinator
                    ) {
                        thread.state = ThreadRuntimeState::Idle;
                    }
                    thread.heartbeat_at = now;
                    thread.clone()
                })
            } else {
                None
            };
            (record, thread_snapshot)
        };

        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        self.persist_worker_request_snapshot(&record).await;
        self.record_event(
            "worker_request_status_updated",
            Some(actor_thread_id),
            record.task_id.clone(),
            None,
            json!({
                "request_id": &record.request_id,
                "request_thread_id": record.thread_id.to_string(),
                "request_type": &record.request_type,
                "status": format!("{:?}", record.status),
            }),
        )
        .await;
        Ok(record)
    }

    pub(crate) async fn read_artifact_blob(
        &self,
        reader_thread_id: ThreadId,
        artifact_id: &str,
        max_bytes: Option<usize>,
    ) -> PraxisResult<ArtifactBlobRead> {
        let requested_max_bytes = max_bytes
            .unwrap_or_else(|| AgentOsPolicy::get().default_artifact_read_max_bytes)
            .clamp(1, HARD_ARTIFACT_READ_MAX_BYTES);
        let (artifact, reader_task_id, max_bytes) = self
            .authorize_artifact_blob_read(reader_thread_id, artifact_id, requested_max_bytes)
            .await?;
        let blob = artifact.metadata.get("blob").ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "artifact `{artifact_id}` has no blob metadata"
            ))
        })?;
        let blob_path = blob
            .get("blob_path")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "artifact `{artifact_id}` has no blob path"
                ))
            })?;
        let blob_bytes = blob.get("blob_bytes").and_then(|value| value.as_u64());
        let path = self.validated_artifact_blob_path(blob_path).await?;
        let mut file = tokio::fs::File::open(path.as_path()).await.map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to open artifact `{artifact_id}` blob: {err}"
            ))
        })?;
        let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
        let mut limited_file = file.take(max_bytes as u64);
        limited_file.read_to_end(&mut bytes).await.map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to read artifact `{artifact_id}` blob: {err}"
            ))
        })?;
        let truncated = blob_bytes
            .map(|total| total > bytes.len() as u64)
            .unwrap_or(bytes.len() == max_bytes);
        let content = String::from_utf8_lossy(&bytes).to_string();
        self.record_artifact_read_budget(reader_task_id.as_str(), bytes.len() as u64)
            .await?;
        self.record_event(
            "artifact_blob_read",
            Some(reader_thread_id),
            Some(reader_task_id.clone()),
            None,
            json!({
                "artifact_id": &artifact.artifact_id,
                "artifact_owner_thread_id": artifact.owner_thread_id.to_string(),
                "artifact_task_id": &artifact.task_id,
                "bytes_read": bytes.len(),
                "blob_bytes": blob_bytes,
                "truncated": truncated,
            }),
        )
        .await;
        Ok(ArtifactBlobRead {
            artifact,
            content,
            bytes_read: bytes.len(),
            blob_bytes,
            truncated,
        })
    }

    async fn authorize_artifact_blob_read(
        &self,
        reader_thread_id: ThreadId,
        artifact_id: &str,
        requested_max_bytes: usize,
    ) -> PraxisResult<(ArtifactRecord, String, usize)> {
        let state = self.state.read().await;
        let reader = state.threads.get(&reader_thread_id).ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS reader thread `{reader_thread_id}`"
            ))
        })?;
        let reader_task_id = reader.current_task_id.clone().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "artifact read rejected: reader thread has no current task_id".to_string(),
            )
        })?;
        let reader_task = state.tasks.get(&reader_task_id).ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "artifact read rejected: current task `{reader_task_id}` is not registered"
            ))
        })?;
        let artifact = state.artifacts.get(artifact_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!("unknown artifact `{artifact_id}`"))
        })?;
        let owner_scope_matches = state
            .threads
            .get(&artifact.owner_thread_id)
            .is_some_and(|owner| owner.coordination_scope == reader.coordination_scope);
        if !owner_scope_matches && artifact.owner_thread_id != reader_thread_id {
            return Err(PraxisErr::UnsupportedOperation(
                "artifact read rejected: artifact owner is outside reader coordination scope"
                    .to_string(),
            ));
        }
        let artifact_ref_allowed = reader_task
            .artifact_refs
            .iter()
            .any(|reference| reference == artifact_id || reference == &artifact.uri);
        let same_task = artifact.task_id == reader_task_id;
        let coordinator = reader.rank == COORDINATOR_RANK;
        if !same_task && !artifact_ref_allowed && !reader_task.exploratory && !coordinator {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "artifact read rejected: artifact `{artifact_id}` is not in task artifact_refs"
            )));
        }
        let max_bytes = if let Some(token_budget) = reader_task.token_budget {
            let remaining = token_budget.saturating_sub(reader_task.artifact_read_bytes);
            if remaining == 0 {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "artifact read rejected: task `{reader_task_id}` token budget is exhausted"
                )));
            }
            requested_max_bytes.min(remaining as usize)
        } else {
            requested_max_bytes
        };
        Ok((artifact, reader_task_id, max_bytes.max(1)))
    }

    async fn record_artifact_read_budget(
        &self,
        task_id: &str,
        bytes_read: u64,
    ) -> PraxisResult<()> {
        if bytes_read == 0 {
            return Ok(());
        }
        let task_snapshot = {
            let mut state = self.state.write().await;
            let task = state.tasks.get_mut(task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "artifact read budget rejected: task `{task_id}` is not registered"
                ))
            })?;
            task.artifact_read_bytes = task.artifact_read_bytes.saturating_add(bytes_read);
            task.updated_at = Utc::now();
            task.clone()
        };
        self.persist_task_snapshot(&task_snapshot).await;
        Ok(())
    }

    pub(crate) async fn request_command_ticket(
        &self,
        thread_id: ThreadId,
        command: &[String],
        cwd: &Path,
    ) -> PraxisResult<ExecutionTicket> {
        self.expire_leases().await;
        self.expire_intent_plans().await;
        let intent = classify_command(command, cwd);
        let now = Utc::now();
        let command_fingerprint = action_fingerprint(command, cwd, intent.kind);
        let ticket_id = format!("exec-ticket-{}", Uuid::new_v4());
        let (thread, task, profile, coordinator_epoch, coordinator_fencing, intent_plan_id) = {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "AgentOS thread `{thread_id}` is not registered"
                ))
            })?;
            let task_id = thread.current_task_id.clone().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(
                    "side-effectful action rejected: thread has no current task_id".to_string(),
                )
            })?;
            let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "current task `{task_id}` is not registered"
                ))
            })?;
            let profile = state
                .profiles
                .get(&thread.profile_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown capability profile `{}`",
                        thread.profile_id
                    ))
                })?;
            let active = state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned();
            if active
                .as_ref()
                .is_some_and(|active| active.expires_at <= now)
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "execution ticket rejected: active coordinator lease has expired".to_string(),
                ));
            }
            let intent_plan_id = Self::find_matching_intent_plan_locked(
                &state,
                thread_id,
                task.task_id.as_str(),
                intent.kind,
                command_fingerprint.as_str(),
                cwd,
            )
            .map(|plan| plan.plan_id.clone());
            (
                thread,
                task,
                profile,
                active.as_ref().map(|value| value.epoch).unwrap_or(0),
                active
                    .as_ref()
                    .map(|value| value.fencing_token)
                    .unwrap_or(0),
                intent_plan_id,
            )
        };

        profile
            .validate_command_intent(&intent, command, cwd)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let intent_plan_id = intent_plan_id.ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "execution ticket rejected: command has no matching AgentOS intent preflight plan"
                    .to_string(),
            )
        })?;
        let ticket_intent_plan_id = intent_plan_id.clone();

        let lease_ids = match self
            .acquire_required_leases(
                thread_id,
                task.task_id.as_str(),
                thread.priority.max(task.priority),
                &intent.required_resources,
            )
            .await
        {
            Ok(lease_ids) => lease_ids,
            Err(err) => {
                self.mark_thread_state(thread_id, ThreadRuntimeState::WaitingForLease)
                    .await;
                return Err(err);
            }
        };

        let ticket = ExecutionTicket {
            ticket_id,
            task_id: task.task_id,
            thread_id,
            coordination_scope: thread.coordination_scope,
            allowed_intent: intent.kind,
            intent_plan_id: Some(intent_plan_id),
            command_fingerprint,
            cwd: cwd.to_path_buf(),
            risk_level: intent.risk_level.clone(),
            capabilities: required_capabilities,
            lease_ids,
            file_scopes: profile.path_scopes.allow.clone(),
            token_budget: task.token_budget,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
            fencing_token: coordinator_fencing,
            coordinator_epoch,
            created_at: now,
        };

        let plan_snapshot_result = {
            let mut state = self.state.write().await;
            match state.intent_plans.get_mut(ticket_intent_plan_id.as_str()) {
                Some(plan)
                    if plan.status != CommandIntentPlanStatus::Pending
                        || plan.expires_at <= now =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "execution ticket rejected: intent plan `{ticket_intent_plan_id}` is not pending"
                    )))
                }
                Some(plan)
                    if plan.thread_id != ticket.thread_id
                        || plan.task_id != ticket.task_id
                        || plan.intent != ticket.allowed_intent
                        || plan.command_fingerprint != ticket.command_fingerprint
                        || normalize_path_for_scope(plan.cwd.as_path())
                            != normalize_path_for_scope(ticket.cwd.as_path()) =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "execution ticket rejected: intent plan `{ticket_intent_plan_id}` does not match ticket action"
                    )))
                }
                Some(plan) => {
                    plan.status = CommandIntentPlanStatus::Consumed;
                    plan.consumed_by_ticket_id = Some(ticket.ticket_id.clone());
                    let plan_snapshot = plan.clone();
                    state
                        .tickets
                        .insert(ticket.ticket_id.clone(), ticket.clone());
                    Ok(plan_snapshot)
                }
                None => Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing intent plan `{ticket_intent_plan_id}`"
                ))),
            }
        };
        let plan_snapshot_result = match plan_snapshot_result {
            Ok(plan) => plan,
            Err(err) => {
                self.release_leases(&ticket.lease_ids).await;
                return Err(err);
            }
        };
        self.persist_ticket_snapshot(&ticket).await;
        self.persist_intent_plan_snapshot(&plan_snapshot_result)
            .await;
        self.record_event(
            "ticket_issued",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "intent": ticket.allowed_intent.as_str(),
                "leases": &ticket.lease_ids,
            }),
        )
        .await;
        self.record_event(
            "command_intent_plan_consumed",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "plan_id": plan_snapshot_result.plan_id,
                "ticket_id": &ticket.ticket_id,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
        Ok(ticket)
    }

    pub(crate) async fn request_mutating_tool_ticket(
        &self,
        thread_id: ThreadId,
        tool_name: &str,
        arguments_fingerprint_source: &str,
    ) -> PraxisResult<ExecutionTicket> {
        self.expire_leases().await;
        self.expire_intent_plans().await;
        let intent = classify_mutating_tool(tool_name);
        let now = Utc::now();
        let action = vec![
            format!("tool:{tool_name}"),
            arguments_fingerprint_source.to_string(),
        ];
        let (
            thread,
            task,
            profile,
            coordinator_epoch,
            coordinator_fencing,
            intent_plan_id,
            command_fingerprint,
        ) = {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "AgentOS thread `{thread_id}` is not registered"
                ))
            })?;
            let task_id = thread.current_task_id.clone().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(
                    "side-effectful tool rejected: thread has no current task_id".to_string(),
                )
            })?;
            let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "current task `{task_id}` is not registered"
                ))
            })?;
            let profile = state
                .profiles
                .get(&thread.profile_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown capability profile `{}`",
                        thread.profile_id
                    ))
                })?;
            let active = state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned();
            if active
                .as_ref()
                .is_some_and(|active| active.expires_at <= now)
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "tool ticket rejected: active coordinator lease has expired".to_string(),
                ));
            }
            let command_fingerprint =
                action_fingerprint(&action, thread.cwd.as_path(), intent.kind);
            let intent_plan_id = Self::find_matching_intent_plan_locked(
                &state,
                thread_id,
                task.task_id.as_str(),
                intent.kind,
                command_fingerprint.as_str(),
                thread.cwd.as_path(),
            )
            .map(|plan| plan.plan_id.clone());
            (
                thread,
                task,
                profile,
                active.as_ref().map(|value| value.epoch).unwrap_or(0),
                active
                    .as_ref()
                    .map(|value| value.fencing_token)
                    .unwrap_or(0),
                intent_plan_id,
                command_fingerprint,
            )
        };

        profile
            .validate_tool_intent(&intent)
            .map_err(PraxisErr::UnsupportedOperation)?;
        let required_capabilities = profile.capability_names_for_action(&intent);
        validate_task_action_contract(&task, &required_capabilities, &intent.required_resources)?;
        let intent_plan_id = intent_plan_id.ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "tool ticket rejected: tool has no matching AgentOS intent preflight plan"
                    .to_string(),
            )
        })?;
        let ticket_intent_plan_id = intent_plan_id.clone();

        let lease_ids = match self
            .acquire_required_leases(
                thread_id,
                task.task_id.as_str(),
                thread.priority.max(task.priority),
                &intent.required_resources,
            )
            .await
        {
            Ok(lease_ids) => lease_ids,
            Err(err) => {
                self.mark_thread_state(thread_id, ThreadRuntimeState::WaitingForLease)
                    .await;
                return Err(err);
            }
        };

        let ticket = ExecutionTicket {
            ticket_id: format!("exec-ticket-{}", Uuid::new_v4()),
            task_id: task.task_id,
            thread_id,
            coordination_scope: thread.coordination_scope,
            allowed_intent: intent.kind,
            intent_plan_id: Some(intent_plan_id),
            command_fingerprint,
            cwd: thread.cwd,
            risk_level: intent.risk_level.clone(),
            capabilities: required_capabilities,
            lease_ids,
            file_scopes: profile.path_scopes.allow.clone(),
            token_budget: task.token_budget,
            expires_at: now + AgentOsPolicy::get().ticket_ttl(),
            fencing_token: coordinator_fencing,
            coordinator_epoch,
            created_at: now,
        };

        let plan_snapshot_result = {
            let mut state = self.state.write().await;
            match state.intent_plans.get_mut(ticket_intent_plan_id.as_str()) {
                Some(plan)
                    if plan.status != CommandIntentPlanStatus::Pending
                        || plan.expires_at <= now =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "tool ticket rejected: intent plan `{ticket_intent_plan_id}` is not pending"
                    )))
                }
                Some(plan)
                    if plan.thread_id != ticket.thread_id
                        || plan.task_id != ticket.task_id
                        || plan.intent != ticket.allowed_intent
                        || plan.command_fingerprint != ticket.command_fingerprint
                        || normalize_path_for_scope(plan.cwd.as_path())
                            != normalize_path_for_scope(ticket.cwd.as_path()) =>
                {
                    Err(PraxisErr::UnsupportedOperation(format!(
                        "tool ticket rejected: intent plan `{ticket_intent_plan_id}` does not match ticket action"
                    )))
                }
                Some(plan) => {
                    plan.status = CommandIntentPlanStatus::Consumed;
                    plan.consumed_by_ticket_id = Some(ticket.ticket_id.clone());
                    let plan_snapshot = plan.clone();
                    state
                        .tickets
                        .insert(ticket.ticket_id.clone(), ticket.clone());
                    Ok(plan_snapshot)
                }
                None => Err(PraxisErr::UnsupportedOperation(format!(
                    "tool ticket references missing intent plan `{ticket_intent_plan_id}`"
                ))),
            }
        };
        let plan_snapshot_result = match plan_snapshot_result {
            Ok(plan) => plan,
            Err(err) => {
                self.release_leases(&ticket.lease_ids).await;
                return Err(err);
            }
        };
        self.persist_ticket_snapshot(&ticket).await;
        self.persist_intent_plan_snapshot(&plan_snapshot_result)
            .await;
        self.record_event(
            "tool_ticket_issued",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "tool": tool_name,
                "intent": ticket.allowed_intent.as_str(),
                "leases": &ticket.lease_ids,
            }),
        )
        .await;
        self.record_event(
            "command_intent_plan_consumed",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "plan_id": plan_snapshot_result.plan_id,
                "ticket_id": &ticket.ticket_id,
                "tool": tool_name,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
        Ok(ticket)
    }

    pub(crate) async fn finish_tool_ticket(
        &self,
        ticket: &ExecutionTicket,
        success: bool,
    ) -> PraxisResult<()> {
        let removed_ticket = {
            let mut state = self.state.write().await;
            state.tickets.remove(ticket.ticket_id.as_str())
        };
        if removed_ticket.is_none() {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "tool ticket `{}` is not live",
                ticket.ticket_id
            )));
        }
        let lease_ids = ticket.lease_ids.clone();
        self.release_leases(&lease_ids).await;
        self.persist_finished_ticket_snapshot(ticket, Some(success))
            .await;
        self.record_event(
            "tool_ticket_finished",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": ticket.ticket_id,
                "success": success,
            }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn start_managed_command(
        self: &Arc<Self>,
        thread_id: ThreadId,
        command: String,
        argv: &[String],
        cwd: &Path,
        process_id: Option<i32>,
    ) -> PraxisResult<ManagedCommandSpan> {
        self.start_managed_command_with_runtime_kind(
            thread_id, command, argv, cwd, process_id, None, None,
        )
        .await
    }

    pub(crate) async fn start_managed_command_with_runtime_kind(
        self: &Arc<Self>,
        thread_id: ThreadId,
        command: String,
        argv: &[String],
        cwd: &Path,
        process_id: Option<i32>,
        runtime_kind: Option<&str>,
        runtime_owner_id: Option<&str>,
    ) -> PraxisResult<ManagedCommandSpan> {
        let ticket = self.request_command_ticket(thread_id, argv, cwd).await?;
        let command_id = match self
            .begin_managed_command(
                &ticket,
                command,
                argv,
                cwd.to_path_buf(),
                process_id,
                runtime_kind.map(str::to_string),
                runtime_owner_id.map(str::to_string),
            )
            .await
        {
            Ok(command_id) => command_id,
            Err(err) => {
                self.revoke_unstarted_ticket(&ticket, err.to_string()).await;
                return Err(err);
            }
        };
        Ok(ManagedCommandSpan {
            agent_os: Arc::clone(self),
            command_id,
        })
    }

    async fn begin_managed_command(
        &self,
        ticket: &ExecutionTicket,
        command: String,
        argv: &[String],
        cwd: PathBuf,
        process_id: Option<i32>,
        runtime_kind: Option<String>,
        runtime_owner_id: Option<String>,
    ) -> PraxisResult<String> {
        let now = Utc::now();
        let command_id = format!("cmd-{}", Uuid::new_v4());
        let command_fingerprint = action_fingerprint(argv, &cwd, ticket.allowed_intent);
        if command_fingerprint != ticket.command_fingerprint {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket command fingerprint does not match requested command".to_string(),
            ));
        }
        if normalize_path_for_scope(&cwd) != normalize_path_for_scope(&ticket.cwd) {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket cwd does not match requested command".to_string(),
            ));
        }
        let baseline_dirty_files = if requires_dirty_audit(ticket.allowed_intent) {
            audit_git_dirty_files(cwd.as_path()).await
        } else {
            Vec::new()
        };
        let baseline_dirty_fingerprints =
            dirty_file_fingerprints(cwd.as_path(), &baseline_dirty_files);
        let record = CommandRecord {
            command_id: command_id.clone(),
            ticket_id: ticket.ticket_id.clone(),
            task_id: ticket.task_id.clone(),
            thread_id: ticket.thread_id,
            intent: ticket.allowed_intent,
            intent_plan_id: ticket.intent_plan_id.clone(),
            command_fingerprint,
            raw_command: command,
            cwd,
            process_id,
            runtime_kind: runtime_kind.clone(),
            runtime_owner_id: runtime_owner_id.clone(),
            started_at: now,
            ended_at: None,
            exit_code: None,
            lease_ids: ticket.lease_ids.clone(),
            artifacts: Vec::new(),
            baseline_dirty_files,
            baseline_dirty_fingerprints,
            dirty_files: Vec::new(),
        };

        let lease_snapshots = {
            let mut state = self.state.write().await;
            self.validate_ticket_locked(&state, ticket)?;
            if let Some(thread) = state.threads.get_mut(&ticket.thread_id) {
                thread.current_command_id = Some(command_id.clone());
                thread.state = ThreadRuntimeState::Running;
                thread.heartbeat_at = now;
            }
            if let Some(task) = state.tasks.get_mut(&ticket.task_id) {
                task.status = TaskStatus::Running;
                task.updated_at = now;
            }
            let mut lease_snapshots = Vec::new();
            for lease_id in &ticket.lease_ids {
                if let Some(lease) = state.leases.get_mut(lease_id) {
                    lease.command_id = Some(command_id.clone());
                    lease.process_id = process_id;
                    lease.runtime_owner_id = runtime_owner_id.clone();
                    lease_snapshots.push(lease.clone());
                }
            }
            if let Some(process_id) = process_id {
                let runtime_kind = runtime_kind
                    .clone()
                    .unwrap_or_else(|| runtime_kind_for_intent(ticket.allowed_intent).to_string());
                let process = ManagedProcessRecord {
                    process_id,
                    command_id: command_id.clone(),
                    task_id: ticket.task_id.clone(),
                    thread_id: ticket.thread_id,
                    cwd: record.cwd.clone(),
                    runtime_kind,
                    runtime_owner_id: runtime_owner_id.clone(),
                    started_at: now,
                    last_heartbeat: now,
                    ended_at: None,
                    status: ManagedProcessStatus::Running,
                };
                let process_key =
                    process_registry_key(process_id, process.runtime_owner_id.as_deref());
                state.processes.insert(process_key, process);
            }
            state.commands.insert(command_id.clone(), record.clone());
            lease_snapshots
        };

        for lease in lease_snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
        self.persist_started_ticket_snapshot(ticket, command_id.as_str())
            .await;
        self.persist_command_snapshot(&record).await;
        if let Some(process_id) = process_id
            && let Some(process) = self
                .process_snapshot(process_id, runtime_owner_id.as_deref())
                .await
        {
            self.persist_process_snapshot(&process).await;
        }
        self.record_event(
            "command_started",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            Some(command_id.clone()),
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
        Ok(command_id)
    }

    async fn attach_process_to_managed_command(
        &self,
        command_id: &str,
        process_id: i32,
    ) -> PraxisResult<()> {
        let now = Utc::now();
        let (command_snapshot, process_snapshot, lease_snapshots) = {
            let mut state = self.state.write().await;
            let command = state.commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            if let Some(existing_process_id) = command.process_id {
                if existing_process_id == process_id {
                    return Ok(());
                }
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "command `{command_id}` already has process id `{existing_process_id}`"
                )));
            }

            command.process_id = Some(process_id);
            let runtime_kind = command
                .runtime_kind
                .clone()
                .unwrap_or_else(|| runtime_kind_for_intent(command.intent).to_string());
            let runtime_owner_id = command.runtime_owner_id.clone();
            let command_snapshot = command.clone();

            let mut lease_snapshots = Vec::new();
            for lease_id in &command_snapshot.lease_ids {
                if let Some(lease) = state.leases.get_mut(lease_id) {
                    lease.process_id = Some(process_id);
                    lease.runtime_owner_id = runtime_owner_id.clone();
                    lease_snapshots.push(lease.clone());
                }
            }

            let process = ManagedProcessRecord {
                process_id,
                command_id: command_id.to_string(),
                task_id: command_snapshot.task_id.clone(),
                thread_id: command_snapshot.thread_id,
                cwd: command_snapshot.cwd.clone(),
                runtime_kind,
                runtime_owner_id,
                started_at: now,
                last_heartbeat: now,
                ended_at: None,
                status: ManagedProcessStatus::Running,
            };
            let process_key = process_registry_key(process_id, process.runtime_owner_id.as_deref());
            state.processes.insert(process_key, process.clone());
            (command_snapshot, process, lease_snapshots)
        };

        for lease in lease_snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
        self.persist_command_snapshot(&command_snapshot).await;
        self.persist_process_snapshot(&process_snapshot).await;
        self.record_event(
            "command_process_attached",
            Some(command_snapshot.thread_id),
            Some(command_snapshot.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "process_id": process_id,
                "runtime_kind": process_snapshot.runtime_kind,
                "runtime_owner_id": process_snapshot.runtime_owner_id,
            }),
        )
        .await;
        Ok(())
    }

    async fn finish_managed_command(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        raw_output: &[u8],
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        self.finish_managed_command_with_output_source(
            command_id,
            exit_code,
            ManagedCommandOutputSource::Bytes(raw_output),
            release_leases,
        )
        .await
    }

    async fn finish_managed_command_with_spooled_output(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        output_spool: ExecOutputSpool,
        fallback_raw_output: &[u8],
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        self.finish_managed_command_with_output_source(
            command_id,
            exit_code,
            ManagedCommandOutputSource::Spool {
                spool: output_spool,
                fallback_raw_output,
            },
            release_leases,
        )
        .await
    }

    async fn finish_managed_command_with_output_source(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        output_source: ManagedCommandOutputSource<'_>,
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        let now = Utc::now();
        let (mut command, mut thread_snapshot, mut task_snapshot, lease_ids) = {
            let mut state = self.state.write().await;
            let (command_snapshot, process_ref) = {
                let command = state.commands.get_mut(command_id).ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
                })?;
                command.ended_at = Some(now);
                command.exit_code = exit_code;
                let process_ref = command
                    .process_id
                    .map(|process_id| (process_id, command.runtime_owner_id.clone()));
                (command.clone(), process_ref)
            };
            if let Some((process_id, runtime_owner_id)) = process_ref {
                let process_key = process_registry_key(process_id, runtime_owner_id.as_deref());
                if let Some(process) = state.processes.get_mut(process_key.as_str()) {
                    process.last_heartbeat = now;
                    process.ended_at = Some(now);
                    process.status = ManagedProcessStatus::Finished;
                }
            }
            let has_active_runtime_command = has_active_assign_runtime_command_locked(
                &state,
                command_snapshot.thread_id,
                command_snapshot.task_id.as_str(),
            );
            let lease_ids = command_snapshot.lease_ids.clone();
            let thread_snapshot =
                if let Some(thread) = state.threads.get_mut(&command_snapshot.thread_id) {
                    if thread.current_command_id.as_deref() == Some(command_id) {
                        thread.current_command_id = None;
                    }
                    if has_active_runtime_command {
                        thread.current_task_id = Some(command_snapshot.task_id.clone());
                        if !matches!(
                            thread.state,
                            ThreadRuntimeState::WaitingForLease
                                | ThreadRuntimeState::WaitingForCoordinator
                                | ThreadRuntimeState::Stopping
                                | ThreadRuntimeState::Stopped
                                | ThreadRuntimeState::Failed
                                | ThreadRuntimeState::Completed
                        ) {
                            thread.state = ThreadRuntimeState::Running;
                        }
                    } else {
                        thread.state = ThreadRuntimeState::Idle;
                    }
                    thread.heartbeat_at = now;
                    Some(thread.clone())
                } else {
                    None
                };
            let task_snapshot = if let Some(task) = state.tasks.get_mut(&command_snapshot.task_id) {
                if !matches!(
                    task.status,
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
                ) {
                    task.status = if has_active_runtime_command {
                        TaskStatus::Running
                    } else {
                        TaskStatus::Assigned
                    };
                }
                task.updated_at = now;
                Some(task.clone())
            } else {
                None
            };
            (command_snapshot, thread_snapshot, task_snapshot, lease_ids)
        };

        let artifact_id = if output_source.is_empty() {
            None
        } else {
            let artifact_result = self
                .create_command_output_artifact(&command, command_id, exit_code, &output_source)
                .await;
            if let ManagedCommandOutputSource::Spool { spool, .. } = &output_source {
                spool.cleanup().await;
            }
            Some(artifact_result?)
        };

        if let Some(artifact_id) = artifact_id.clone() {
            command.artifacts.push(artifact_id.clone());
            let mut state = self.state.write().await;
            state
                .commands
                .insert(command_id.to_string(), command.clone());
        }

        if let Some(outcome) = self.audit_finished_command_dirty_files(&command).await? {
            let dirty_file_report =
                format_dirty_file_report(&outcome.dirty_files, outcome.violation_path.as_ref());
            let dirty_file_artifact_id = self
                .create_blob_artifact(
                    outcome.command.task_id.clone(),
                    outcome.command.thread_id,
                    ArtifactType::DirtyFileReport,
                    "dirty-file-report",
                    dirty_file_report.clone(),
                    json!({
                        "command_id": command_id,
                        "dirty_files": outcome
                            .dirty_files
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>(),
                        "violation_path": outcome
                            .violation_path
                            .as_ref()
                            .map(|path| path.display().to_string()),
                    }),
                    "txt",
                    dirty_file_report.as_bytes(),
                )
                .await?;
            command = outcome.command;
            command.artifacts.push(dirty_file_artifact_id.clone());
            {
                let mut state = self.state.write().await;
                state
                    .commands
                    .insert(command_id.to_string(), command.clone());
            }
            thread_snapshot = outcome.thread_snapshot.or(thread_snapshot);
            task_snapshot = outcome.task_snapshot.or(task_snapshot);
            self.record_event(
                "command_dirty_file_audit",
                Some(command.thread_id),
                Some(command.task_id.clone()),
                Some(command.command_id.clone()),
                json!({
                    "artifact_id": dirty_file_artifact_id,
                    "dirty_files": command
                        .dirty_files
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>(),
                    "violation_path": outcome
                        .violation_path
                        .as_ref()
                        .map(|path| path.display().to_string()),
                }),
            )
            .await;
        }

        if release_leases {
            self.release_leases(&lease_ids).await;
        }
        let finished_ticket = {
            let mut state = self.state.write().await;
            state.tickets.remove(command.ticket_id.as_str())
        };
        if let Some(ticket) = finished_ticket.as_ref() {
            self.persist_finished_ticket_snapshot(ticket, None).await;
        }
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if let Some(task) = task_snapshot {
            self.persist_task_snapshot(&task).await;
        }
        self.persist_command_snapshot(&command).await;
        if let Some(process_id) = command.process_id
            && let Some(process) = self
                .process_snapshot(process_id, command.runtime_owner_id.as_deref())
                .await
        {
            self.persist_process_snapshot(&process).await;
        }
        self.record_event(
            "command_finished",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "exit_code": exit_code,
                "artifact_id": artifact_id,
                "leases_released": release_leases,
                "runtime_kind": command.runtime_kind.as_deref(),
                "runtime_owner_id": command.runtime_owner_id.as_deref(),
                "process_id": command.process_id,
            }),
        )
        .await;
        Ok(artifact_id)
    }

    async fn revoke_unstarted_ticket(&self, ticket: &ExecutionTicket, reason: String) {
        {
            let mut state = self.state.write().await;
            state.tickets.remove(ticket.ticket_id.as_str());
        }
        self.release_leases(&ticket.lease_ids).await;
        self.persist_revoked_ticket_snapshot(ticket, reason.as_str())
            .await;
        self.record_event(
            "ticket_revoked",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "reason": reason,
                "stage": "begin_managed_command",
            }),
        )
        .await;
    }

    async fn checkpoint_managed_command(
        &self,
        command_id: &str,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        if raw_output.is_empty() {
            return Ok(None);
        }
        let command = self
            .state
            .read()
            .await
            .commands
            .get(command_id)
            .cloned()
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
        self.renew_command_leases(&command).await;
        let artifact_id = self
            .create_blob_artifact(
                command.task_id.clone(),
                command.thread_id,
                artifact_type_for_intent(command.intent),
                "command-checkpoint",
                summarize_output(raw_output),
                json!({
                    "command_id": command_id,
                    "bytes": raw_output.len(),
                    "checkpoint": true,
                }),
                "log",
                raw_output,
            )
            .await?;
        let command_snapshot = {
            let mut state = self.state.write().await;
            let process_ref = state.commands.get(command_id).and_then(|command| {
                command
                    .process_id
                    .map(|process_id| (process_id, command.runtime_owner_id.clone()))
            });
            if let Some((process_id, runtime_owner_id)) = process_ref {
                let process_key = process_registry_key(process_id, runtime_owner_id.as_deref());
                if let Some(process) = state.processes.get_mut(process_key.as_str()) {
                    process.last_heartbeat = Utc::now();
                }
            }
            if let Some(command) = state.commands.get_mut(command_id) {
                command.artifacts.push(artifact_id.clone());
                Some(command.clone())
            } else {
                None
            }
        };
        if let Some(command) = command_snapshot {
            self.persist_command_snapshot(&command).await;
            if let Some(process_id) = command.process_id
                && let Some(process) = self
                    .process_snapshot(process_id, command.runtime_owner_id.as_deref())
                    .await
            {
                self.persist_process_snapshot(&process).await;
            }
        }
        self.record_event(
            "command_checkpoint",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "artifact_id": artifact_id,
            }),
        )
        .await;
        Ok(Some(artifact_id))
    }

    pub(crate) async fn checkpoint_managed_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self
            .command_id_for_process(process_id, runtime_owner_id)
            .await
        else {
            return Ok(None);
        };
        self.checkpoint_managed_command(command_id.as_str(), raw_output)
            .await
    }

    pub(crate) async fn finish_managed_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        exit_code: Option<i32>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self
            .command_id_for_process(process_id, runtime_owner_id)
            .await
        else {
            return Ok(None);
        };
        self.finish_managed_command(command_id.as_str(), exit_code, raw_output, true)
            .await
    }

    async fn audit_finished_command_dirty_files(
        &self,
        command: &CommandRecord,
    ) -> PraxisResult<Option<DirtyAuditOutcome>> {
        if !requires_dirty_audit(command.intent) {
            return Ok(None);
        }

        let after_dirty_files = audit_git_dirty_files(command.cwd.as_path()).await;
        let dirty_files = dirty_file_delta(
            command.cwd.as_path(),
            &command.baseline_dirty_files,
            &command.baseline_dirty_fingerprints,
            &after_dirty_files,
        );
        if dirty_files.is_empty() {
            return Ok(None);
        }

        let (command_snapshot, thread_snapshot, task_snapshot, violation) = {
            let mut state = self.state.write().await;
            let task_snapshot = state.tasks.get(&command.task_id).cloned();
            let profile = state
                .threads
                .get(&command.thread_id)
                .and_then(|thread| state.profiles.get(thread.profile_id.as_str()))
                .cloned();
            let violation_path = task_snapshot.as_ref().and_then(|task| {
                dirty_files
                    .iter()
                    .find(|path| {
                        !dirty_file_allowed_by_task(task, path)
                            || !profile
                                .as_ref()
                                .is_some_and(|profile| profile.path_scopes.allows(path))
                    })
                    .cloned()
            });

            let command_record = state.commands.get_mut(&command.command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{}`", command.command_id))
            })?;
            push_unique_dirty_files(&mut command_record.dirty_files, &dirty_files);
            let command_snapshot = command_record.clone();

            let (thread_snapshot, task_snapshot) = if violation_path.is_some() {
                let thread_snapshot = state.threads.get_mut(&command.thread_id).map(|thread| {
                    thread.state = ThreadRuntimeState::Failed;
                    thread.heartbeat_at = Utc::now();
                    thread.clone()
                });
                let task_snapshot = state.tasks.get_mut(&command.task_id).map(|task| {
                    task.status = TaskStatus::Failed;
                    task.updated_at = Utc::now();
                    task.clone()
                });
                (thread_snapshot, task_snapshot)
            } else {
                (None, task_snapshot)
            };

            (
                command_snapshot,
                thread_snapshot,
                task_snapshot,
                violation_path,
            )
        };

        self.persist_command_snapshot(&command_snapshot).await;
        if let Some(thread) = thread_snapshot.as_ref() {
            self.persist_thread_snapshot(thread).await;
        }
        if let Some(task) = task_snapshot.as_ref() {
            self.persist_task_snapshot(task).await;
        }

        if let Some(path) = violation.as_ref() {
            self.record_event(
                "policy_violation",
                Some(command_snapshot.thread_id),
                Some(command_snapshot.task_id.clone()),
                Some(command_snapshot.command_id.clone()),
                json!({
                    "reason": "dirty_file_outside_task_or_profile_scope",
                    "path": path.display().to_string(),
                    "detected_by": "post_command_dirty_audit",
                }),
            )
            .await;
        }

        Ok(Some(DirtyAuditOutcome {
            command: command_snapshot,
            thread_snapshot,
            task_snapshot,
            dirty_files,
            violation_path: violation,
        }))
    }

    async fn record_command_dirty_files(
        &self,
        command_id: &str,
        dirty_files: Vec<PathBuf>,
    ) -> PraxisResult<()> {
        if dirty_files.is_empty() {
            return Ok(());
        }
        let violation = {
            let state = self.state.read().await;
            let command = state.commands.get(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            let task = state.tasks.get(&command.task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "command `{command_id}` references unknown task `{}`",
                    command.task_id
                ))
            })?;
            let profile = state
                .threads
                .get(&command.thread_id)
                .and_then(|thread| state.profiles.get(thread.profile_id.as_str()));
            dirty_files
                .iter()
                .find(|path| {
                    !dirty_file_allowed_by_task(task, path)
                        || !profile.is_some_and(|profile| profile.path_scopes.allows(path))
                })
                .map(|path| (command.clone(), task.clone(), path.clone()))
        };
        if let Some((command, task, path)) = violation {
            self.record_event(
                "policy_violation",
                Some(command.thread_id),
                Some(command.task_id.clone()),
                Some(command.command_id.clone()),
                json!({
                    "reason": "dirty_file_outside_task_or_profile_scope",
                    "path": path.display().to_string(),
                    "task_scope": task.scope,
                }),
            )
            .await;
            return Err(PraxisErr::UnsupportedOperation(format!(
                "dirty file `{}` is outside AgentOS task/profile scope",
                path.display()
            )));
        }
        let command = {
            let mut state = self.state.write().await;
            let command = state.commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            push_unique_dirty_files(&mut command.dirty_files, &dirty_files);
            command.clone()
        };
        self.persist_command_snapshot(&command).await;
        self.record_event(
            "command_dirty_files_recorded",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command.command_id.clone()),
            json!({
                "dirty_files": command
                    .dirty_files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
            }),
        )
        .await;
        Ok(())
    }

    async fn acquire_required_leases(
        &self,
        thread_id: ThreadId,
        task_id: &str,
        priority: i32,
        requirements: &[ResourceRequirement],
    ) -> PraxisResult<Vec<String>> {
        let now = Utc::now();
        let mut seen = HashSet::new();
        let planned_requirements = requirements
            .iter()
            .filter(|requirement| seen.insert(requirement.key()))
            .cloned()
            .collect::<Vec<_>>();
        let mut acquired = Vec::new();
        let mut snapshots = Vec::new();
        {
            let mut state = self.state.write().await;
            for requirement in &planned_requirements {
                if let Some(owner) = self.lease_conflict_owner_locked(&state, requirement) {
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "resource lease `{}` is held by {owner}",
                        requirement.key()
                    )));
                }
            }
            state.fencing_counter = state.fencing_counter.saturating_add(1);
            let fencing_token = state.fencing_counter;
            for requirement in planned_requirements {
                let key = requirement.key();
                let lease = ResourceLease {
                    lease_id: format!("lease-{}", Uuid::new_v4()),
                    resource_type: requirement.resource_type().to_string(),
                    scope: key,
                    mode: requirement.mode(),
                    owner_thread_id: thread_id,
                    task_id: task_id.to_string(),
                    priority,
                    fencing_token,
                    created_at: now,
                    expires_at: Some(now + AgentOsPolicy::get().lease_ttl()),
                    revocable: true,
                    metadata: json!({}),
                    command_id: None,
                    process_id: None,
                    runtime_owner_id: None,
                };
                acquired.push(lease.lease_id.clone());
                snapshots.push(lease.clone());
                state.leases.insert(lease.lease_id.clone(), lease);
            }
        }
        for lease in snapshots {
            self.persist_lease_snapshot(&lease).await;
            self.record_event(
                "lease_acquired",
                Some(thread_id),
                Some(task_id.to_string()),
                None,
                json!({
                    "lease_id": lease.lease_id,
                    "resource_type": lease.resource_type,
                    "scope": lease.scope,
                    "mode": lease.mode.as_str(),
                }),
            )
            .await;
        }
        Ok(acquired)
    }

    async fn release_leases(&self, lease_ids: &[String]) {
        let mut released = Vec::new();
        {
            let mut state = self.state.write().await;
            for lease_id in lease_ids {
                if let Some(lease) = state.leases.remove(lease_id) {
                    released.push(lease);
                }
            }
        }
        for lease in released {
            self.record_event(
                "lease_released",
                Some(lease.owner_thread_id),
                Some(lease.task_id),
                None,
                json!({
                    "lease_id": lease.lease_id,
                    "resource_type": lease.resource_type,
                    "scope": lease.scope,
                }),
            )
            .await;
        }
    }

    async fn expire_leases(&self) {
        let now = Utc::now();
        let mut expired = Vec::new();
        {
            let mut state = self.state.write().await;
            let ids: Vec<String> = state
                .leases
                .iter()
                .filter_map(|(lease_id, lease)| {
                    lease
                        .expires_at
                        .is_some_and(|expires_at| expires_at <= now)
                        .then(|| lease_id.clone())
                })
                .collect();
            for lease_id in ids {
                if let Some(lease) = state.leases.remove(&lease_id) {
                    expired.push(lease);
                }
            }
        }
        let mut cleanup_processes = HashSet::new();
        let mut finish_commands = HashSet::new();
        for lease in expired {
            if let Some(process_id) = lease.process_id {
                cleanup_processes.insert((process_id, lease.runtime_owner_id.clone()));
            }
            if let Some(command_id) = lease.command_id.clone() {
                finish_commands.insert(command_id);
            }
            self.record_event(
                "lease_expired",
                Some(lease.owner_thread_id),
                Some(lease.task_id),
                lease.command_id.clone(),
                json!({
                    "lease_id": lease.lease_id,
                    "scope": lease.scope,
                    "process_id": lease.process_id,
                    "runtime_owner_id": lease.runtime_owner_id,
                    "requires_process_cleanup": lease.process_id.is_some(),
                }),
            )
            .await;
        }
        for (process_id, runtime_owner_id) in cleanup_processes {
            self.mark_process_status(
                process_id,
                runtime_owner_id.as_deref(),
                ManagedProcessStatus::Cleaning,
            )
            .await;
            let cleaned = self
                .cleanup_process(process_id, runtime_owner_id.as_deref())
                .await;
            if cleaned {
                self.mark_process_finished(process_id, runtime_owner_id.as_deref())
                    .await;
            }
            self.record_event(
                "lease_process_cleanup",
                None,
                None,
                None,
                json!({
                    "process_id": process_id,
                    "runtime_owner_id": runtime_owner_id,
                    "cleaned": cleaned,
                }),
            )
            .await;
        }
        for command_id in finish_commands {
            let _ = self
                .finish_managed_command(
                    command_id.as_str(),
                    Some(-1),
                    b"command terminated because AgentOS lease expired",
                    /*release_leases*/ false,
                )
                .await;
        }
    }

    async fn expire_tickets(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            let active_ticket_ids = state
                .commands
                .values()
                .filter(|command| command.ended_at.is_none())
                .map(|command| command.ticket_id.clone())
                .collect::<HashSet<_>>();
            let ids = state
                .tickets
                .iter()
                .filter(|(_, ticket)| ticket.expires_at <= now)
                .filter(|(ticket_id, _)| !active_ticket_ids.contains(ticket_id.as_str()))
                .map(|(ticket_id, _)| ticket_id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|ticket_id| state.tickets.remove(ticket_id.as_str()))
                .collect::<Vec<_>>()
        };
        for ticket in expired {
            self.release_leases(&ticket.lease_ids).await;
            self.persist_revoked_ticket_snapshot(&ticket, "ticket expired before completion")
                .await;
            self.record_event(
                "ticket_expired",
                Some(ticket.thread_id),
                Some(ticket.task_id.clone()),
                None,
                json!({
                    "ticket_id": &ticket.ticket_id,
                    "intent_plan_id": &ticket.intent_plan_id,
                    "expires_at": ticket.expires_at.to_rfc3339(),
                }),
            )
            .await;
        }
    }

    async fn cleanup_process(&self, process_id: i32, runtime_owner_id: Option<&str>) -> bool {
        let (runtime_kind, process_owner_id) = {
            let state = self.state.read().await;
            let process_key = process_registry_key(process_id, runtime_owner_id);
            state
                .processes
                .get(process_key.as_str())
                .map(|process| {
                    (
                        Some(process.runtime_kind.clone()),
                        process.runtime_owner_id.clone(),
                    )
                })
                .unwrap_or((None, runtime_owner_id.map(str::to_string)))
        };

        if let (Some(runtime_kind), Some(process_owner_id)) =
            (runtime_kind.as_deref(), process_owner_id.as_deref())
        {
            let exact_key = cleaner_registry_key(runtime_kind, process_owner_id);
            let cleaner = self
                .process_cleaners_by_owner
                .read()
                .await
                .get(exact_key.as_str())
                .cloned();
            if let Some(cleaner) = cleaner {
                if cleaner.cleanup_agent_os_process(process_id).await {
                    return true;
                }
            }
        }

        let cleaners = {
            let cleaners_by_kind = self.process_cleaners.read().await;
            let mut selected = Vec::new();

            // If the process record has an owning backend id, process ids are
            // backend-local. Do not fan out to every same-kind cleaner: that can
            // kill the wrong backend's process when two sessions reuse the same
            // numeric id. A generic cleaner may still handle host-global process
            // ids, but owner-scoped processes require exact routing.
            if process_owner_id.is_none() {
                if let Some(runtime_kind) = runtime_kind.as_deref() {
                    if let Some(cleaners) = cleaners_by_kind.get(runtime_kind) {
                        selected.extend(cleaners.iter().cloned());
                    }
                }
            }
            if let Some(cleaners) = cleaners_by_kind.get(process_runtime_kind::GENERIC) {
                selected.extend(cleaners.iter().cloned());
            }
            if selected.is_empty() && process_owner_id.is_none() {
                selected.extend(cleaners_by_kind.values().flatten().cloned());
            }
            selected
        };
        for cleaner in cleaners {
            if cleaner.cleanup_agent_os_process(process_id).await {
                return true;
            }
        }
        false
    }

    async fn create_command_output_artifact(
        &self,
        command: &CommandRecord,
        command_id: &str,
        exit_code: Option<i32>,
        output_source: &ManagedCommandOutputSource<'_>,
    ) -> PraxisResult<String> {
        let metadata = json!({
            "command_id": command_id,
            "bytes": output_source.byte_len(),
            "exit_code": exit_code,
            "runtime_kind": command.runtime_kind.as_deref(),
            "runtime_owner_id": command.runtime_owner_id.as_deref(),
            "process_id": command.process_id,
        });
        match output_source {
            ManagedCommandOutputSource::Bytes(raw_output) => {
                self.create_blob_artifact(
                    command.task_id.clone(),
                    command.thread_id,
                    artifact_type_for_intent(command.intent),
                    "command-log",
                    output_source.summary(),
                    metadata,
                    "log",
                    raw_output,
                )
                .await
            }
            ManagedCommandOutputSource::Spool { spool, .. } => {
                self.create_blob_artifact_from_spool(
                    command.task_id.clone(),
                    command.thread_id,
                    artifact_type_for_intent(command.intent),
                    "command-log",
                    output_source.summary(),
                    metadata,
                    "log",
                    spool,
                )
                .await
            }
        }
    }

    async fn create_blob_artifact(
        &self,
        task_id: String,
        owner_thread_id: ThreadId,
        artifact_type: ArtifactType,
        uri_namespace: &str,
        summary: String,
        metadata: serde_json::Value,
        extension: &str,
        blob: &[u8],
    ) -> PraxisResult<String> {
        let artifact_id = format!("artifact-{}", Uuid::new_v4());
        let blob_path = self
            .write_artifact_blob(artifact_id.as_str(), extension, blob)
            .await;
        let artifact = ArtifactRecord {
            artifact_id: artifact_id.clone(),
            task_id,
            owner_thread_id,
            artifact_type,
            uri: format!("artifact://{uri_namespace}/{artifact_id}"),
            summary,
            metadata: metadata_with_blob(metadata, blob.len(), blob_path.as_ref()),
            created_at: Utc::now(),
        };
        self.insert_artifact_record(artifact).await
    }

    async fn create_blob_artifact_from_spool(
        &self,
        task_id: String,
        owner_thread_id: ThreadId,
        artifact_type: ArtifactType,
        uri_namespace: &str,
        summary: String,
        metadata: serde_json::Value,
        extension: &str,
        spool: &ExecOutputSpool,
    ) -> PraxisResult<String> {
        let artifact_id = format!("artifact-{}", Uuid::new_v4());
        let blob_path = self
            .write_artifact_blob_from_spool(artifact_id.as_str(), extension, spool)
            .await;
        let artifact = ArtifactRecord {
            artifact_id: artifact_id.clone(),
            task_id,
            owner_thread_id,
            artifact_type,
            uri: format!("artifact://{uri_namespace}/{artifact_id}"),
            summary,
            metadata: metadata_with_blob(metadata, spool.total_bytes(), blob_path.as_ref()),
            created_at: Utc::now(),
        };
        self.insert_artifact_record(artifact).await
    }

    async fn insert_artifact_record(&self, artifact: ArtifactRecord) -> PraxisResult<String> {
        let artifact_id = artifact.artifact_id.clone();
        {
            let mut state = self.state.write().await;
            state
                .artifacts
                .insert(artifact_id.clone(), artifact.clone());
        }
        self.persist_artifact_snapshot(&artifact).await;
        self.record_event(
            "artifact_created",
            Some(artifact.owner_thread_id),
            Some(artifact.task_id.clone()),
            None,
            json!({
                "artifact_id": artifact.artifact_id,
                "type": format!("{:?}", artifact.artifact_type),
                "uri": artifact.uri,
            }),
        )
        .await;
        Ok(artifact_id)
    }

    async fn write_artifact_blob(
        &self,
        artifact_id: &str,
        extension: &str,
        blob: &[u8],
    ) -> Option<PathBuf> {
        let db = self.state_db.read().await.clone()?;
        let root = db.praxis_home().join("artifacts").join("agent-os");
        if let Err(err) = tokio::fs::create_dir_all(root.as_path()).await {
            tracing::warn!("failed to create AgentOS artifact directory: {err}");
            return None;
        }
        let extension = sanitize_artifact_extension(extension);
        let path = root.join(format!("{artifact_id}.{extension}"));
        if let Err(err) = tokio::fs::write(path.as_path(), blob).await {
            tracing::warn!("failed to write AgentOS artifact blob: {err}");
            return None;
        }
        Some(path)
    }

    async fn write_artifact_blob_from_spool(
        &self,
        artifact_id: &str,
        extension: &str,
        spool: &ExecOutputSpool,
    ) -> Option<PathBuf> {
        let db = self.state_db.read().await.clone()?;
        let root = db.praxis_home().join("artifacts").join("agent-os");
        if let Err(err) = tokio::fs::create_dir_all(root.as_path()).await {
            tracing::warn!("failed to create AgentOS artifact directory: {err}");
            return None;
        }
        let extension = sanitize_artifact_extension(extension);
        let path = root.join(format!("{artifact_id}.{extension}"));
        let mut out = match tokio::fs::File::create(path.as_path()).await {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!("failed to create AgentOS artifact blob: {err}");
                return None;
            }
        };
        for stream in [&spool.stdout, &spool.stderr].into_iter().flatten() {
            if let Err(err) = append_spool_stream(&mut out, stream).await {
                tracing::warn!("failed to persist AgentOS artifact spool: {err}");
                let _ = tokio::fs::remove_file(path.as_path()).await;
                return None;
            }
        }
        if let Err(err) = out.flush().await {
            tracing::warn!("failed to flush AgentOS artifact blob: {err}");
            let _ = tokio::fs::remove_file(path.as_path()).await;
            return None;
        }
        Some(path)
    }

    async fn validated_artifact_blob_path(&self, blob_path: &str) -> PraxisResult<PathBuf> {
        let db = self.state_db.read().await.clone().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "AgentOS artifact blob store is unavailable without state DB".to_string(),
            )
        })?;
        let root = db.praxis_home().join("artifacts").join("agent-os");
        let root = std::fs::canonicalize(root.as_path()).map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to resolve AgentOS artifact root: {err}"
            ))
        })?;
        let path = PathBuf::from(blob_path);
        let path = std::fs::canonicalize(path.as_path()).map_err(|err| {
            PraxisErr::UnsupportedOperation(format!("failed to resolve artifact blob path: {err}"))
        })?;
        if !path.starts_with(root.as_path()) {
            return Err(PraxisErr::UnsupportedOperation(
                "artifact blob path escapes AgentOS artifact root".to_string(),
            ));
        }
        Ok(path)
    }

    async fn mark_thread_state(&self, thread_id: ThreadId, state_value: ThreadRuntimeState) {
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(thread) = state.threads.get_mut(&thread_id) else {
                return;
            };
            thread.state = state_value;
            thread.heartbeat_at = Utc::now();
            thread.clone()
        };
        self.persist_thread_snapshot(&snapshot).await;
    }

    async fn command_id_for_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> Option<String> {
        let state = self.state.read().await;
        let process_key = process_registry_key(process_id, runtime_owner_id);
        if let Some(process) = state.processes.get(process_key.as_str())
            && process.status != ManagedProcessStatus::Finished
        {
            return Some(process.command_id.clone());
        }
        state
            .commands
            .values()
            .find(|command| {
                command.process_id == Some(process_id)
                    && command.runtime_owner_id.as_deref() == runtime_owner_id
                    && command.ended_at.is_none()
            })
            .map(|command| command.command_id.clone())
    }

    async fn process_snapshot(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> Option<ManagedProcessRecord> {
        let process_key = process_registry_key(process_id, runtime_owner_id);
        self.state
            .read()
            .await
            .processes
            .get(process_key.as_str())
            .cloned()
    }

    async fn mark_process_status(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        status: ManagedProcessStatus,
    ) {
        let process_key = process_registry_key(process_id, runtime_owner_id);
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(process) = state.processes.get_mut(process_key.as_str()) else {
                return;
            };
            process.status = status;
            process.last_heartbeat = Utc::now();
            process.clone()
        };
        self.persist_process_snapshot(&snapshot).await;
    }

    async fn mark_process_finished(&self, process_id: i32, runtime_owner_id: Option<&str>) {
        let now = Utc::now();
        let process_key = process_registry_key(process_id, runtime_owner_id);
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(process) = state.processes.get_mut(process_key.as_str()) else {
                return;
            };
            process.status = ManagedProcessStatus::Finished;
            process.last_heartbeat = now;
            process.ended_at.get_or_insert(now);
            process.clone()
        };
        self.persist_process_snapshot(&snapshot).await;
    }

    async fn renew_command_leases(&self, command: &CommandRecord) {
        let snapshots = {
            let mut state = self.state.write().await;
            command
                .lease_ids
                .iter()
                .filter_map(|lease_id| {
                    let lease = state.leases.get_mut(lease_id)?;
                    lease.expires_at = Some(Utc::now() + AgentOsPolicy::get().lease_ttl());
                    Some(lease.clone())
                })
                .collect::<Vec<_>>()
        };
        for lease in snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
    }

    fn validate_ticket_locked(
        &self,
        state: &AgentOsState,
        ticket: &ExecutionTicket,
    ) -> PraxisResult<()> {
        if ticket.expires_at <= Utc::now() {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "execution ticket `{}` has expired",
                ticket.ticket_id
            )));
        }
        if let Some(active) = state
            .active_coordinators
            .get(ticket.coordination_scope.as_str())
            && (active.expires_at <= Utc::now()
                || ticket.coordinator_epoch != active.epoch
                || ticket.fencing_token != active.fencing_token)
        {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket coordinator epoch is stale or expired".to_string(),
            ));
        }
        for lease_id in &ticket.lease_ids {
            let lease = state.leases.get(lease_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing lease `{lease_id}`"
                ))
            })?;
            if lease.owner_thread_id != ticket.thread_id || lease.task_id != ticket.task_id {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references lease `{lease_id}` owned by another task or thread"
                )));
            }
        }
        if let Some(plan_id) = ticket.intent_plan_id.as_deref() {
            let plan = state.intent_plans.get(plan_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing intent plan `{plan_id}`"
                ))
            })?;
            if plan.status != CommandIntentPlanStatus::Consumed
                || plan.consumed_by_ticket_id.as_deref() != Some(ticket.ticket_id.as_str())
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket intent plan `{plan_id}` was not consumed by this ticket"
                )));
            }
            if plan.thread_id != ticket.thread_id
                || plan.task_id != ticket.task_id
                || plan.intent != ticket.allowed_intent
                || plan.command_fingerprint != ticket.command_fingerprint
                || normalize_path_for_scope(plan.cwd.as_path())
                    != normalize_path_for_scope(ticket.cwd.as_path())
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket intent plan `{plan_id}` does not match ticket action"
                )));
            }
        }
        Ok(())
    }

    fn lease_conflict_owner_locked(
        &self,
        state: &AgentOsState,
        requirement: &ResourceRequirement,
    ) -> Option<String> {
        let key = requirement.key();
        let mode = requirement.mode();
        match mode {
            LeaseMode::Advisory | LeaseMode::Shared => None,
            LeaseMode::Capacity => {
                let capacity = capacity_for_requirement(requirement);
                let active = state
                    .leases
                    .values()
                    .filter(|lease| lease.scope == key)
                    .filter(|lease| {
                        lease
                            .expires_at
                            .is_none_or(|expires_at| expires_at > Utc::now())
                    })
                    .collect::<Vec<_>>();
                (active.len() >= capacity).then(|| {
                    active
                        .first()
                        .map(|lease| lease.owner_thread_id.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                })
            }
            LeaseMode::Exclusive => state
                .leases
                .values()
                .find(|lease| {
                    lease.scope == key
                        && lease
                            .expires_at
                            .is_none_or(|expires_at| expires_at > Utc::now())
                })
                .map(|lease| lease.owner_thread_id.to_string()),
        }
    }

    fn find_matching_intent_plan_locked<'a>(
        state: &'a AgentOsState,
        thread_id: ThreadId,
        task_id: &str,
        intent: ActionIntentKind,
        command_fingerprint: &str,
        cwd: &Path,
    ) -> Option<&'a CommandIntentPlan> {
        let cwd = normalize_path_for_scope(cwd);
        let now = Utc::now();
        state
            .intent_plans
            .values()
            .filter(|plan| plan.status == CommandIntentPlanStatus::Pending)
            .filter(|plan| plan.expires_at > now)
            .filter(|plan| plan.thread_id == thread_id)
            .filter(|plan| plan.task_id == task_id)
            .filter(|plan| plan.intent == intent)
            .filter(|plan| plan.command_fingerprint == command_fingerprint)
            .filter(|plan| normalize_path_for_scope(plan.cwd.as_path()) == cwd)
            .max_by_key(|plan| plan.created_at)
    }

    async fn insert_intent_plan(&self, plan: &CommandIntentPlan) {
        let superseded_plans = {
            let mut state = self.state.write().await;
            let normalized_cwd = normalize_path_for_scope(plan.cwd.as_path());
            let superseded_plans = state
                .intent_plans
                .values_mut()
                .filter(|existing| existing.status == CommandIntentPlanStatus::Pending)
                .filter(|existing| existing.thread_id == plan.thread_id)
                .filter(|existing| existing.task_id == plan.task_id.as_str())
                .filter(|existing| existing.intent == plan.intent)
                .filter(|existing| {
                    existing.command_fingerprint == plan.command_fingerprint.as_str()
                })
                .filter(|existing| {
                    normalize_path_for_scope(existing.cwd.as_path()) == normalized_cwd
                })
                .map(|existing| {
                    existing.status = CommandIntentPlanStatus::Rejected;
                    existing.clone()
                })
                .collect::<Vec<_>>();
            state
                .intent_plans
                .insert(plan.plan_id.clone(), plan.clone());
            superseded_plans
        };
        for superseded in superseded_plans {
            self.persist_intent_plan_snapshot(&superseded).await;
            self.record_event(
                "command_intent_plan_superseded",
                Some(superseded.thread_id),
                Some(superseded.task_id.clone()),
                None,
                json!({
                    "plan_id": &superseded.plan_id,
                    "replaced_by_plan_id": &plan.plan_id,
                    "intent": superseded.intent.as_str(),
                }),
            )
            .await;
        }
    }

    async fn expire_intent_plans(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            state
                .intent_plans
                .values_mut()
                .filter(|plan| plan.status == CommandIntentPlanStatus::Pending)
                .filter(|plan| plan.expires_at <= now)
                .map(|plan| {
                    plan.status = CommandIntentPlanStatus::Expired;
                    plan.clone()
                })
                .collect::<Vec<_>>()
        };
        for plan in expired {
            self.persist_intent_plan_snapshot(&plan).await;
            self.record_event(
                "command_intent_plan_expired",
                Some(plan.thread_id),
                Some(plan.task_id.clone()),
                None,
                json!({
                    "plan_id": &plan.plan_id,
                    "intent": plan.intent.as_str(),
                    "expires_at": plan.expires_at.to_rfc3339(),
                }),
            )
            .await;
        }
    }

    async fn expire_runtime_commands(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            state
                .runtime_commands
                .values_mut()
                .filter(|command| {
                    matches!(
                        command.status,
                        RuntimeCommandStatus::Pending
                            | RuntimeCommandStatus::Acked
                            | RuntimeCommandStatus::Executing
                    )
                })
                .filter(|command| command.expires_at <= now)
                .map(|command| {
                    command.status = RuntimeCommandStatus::Expired;
                    command.updated_at = now;
                    command.clone()
                })
                .collect::<Vec<_>>()
        };
        for command in expired {
            self.persist_runtime_command_snapshot(&command).await;
            self.record_event(
                "runtime_command_status_updated",
                Some(command.to_thread_id),
                command.task_id.clone(),
                None,
                json!({
                    "command_id": &command.command_id,
                    "from_thread_id": command.from_thread_id.to_string(),
                    "to_thread_id": command.to_thread_id.to_string(),
                    "command_type": command.command_type.as_str(),
                    "status": format!("{:?}", command.status),
                    "source": "expire_runtime_commands",
                }),
            )
            .await;
        }
    }

    async fn record_event(
        &self,
        event_type: &str,
        thread_id: Option<ThreadId>,
        task_id: Option<String>,
        command_id: Option<String>,
        payload: serde_json::Value,
    ) {
        let entry = EventLedgerEntry {
            event_id: format!("event-{}", Uuid::new_v4()),
            event_type: event_type.to_string(),
            thread_id,
            task_id,
            command_id,
            payload,
            created_at: Utc::now(),
        };
        {
            let mut state = self.state.write().await;
            state.events.push(entry.clone());
            let max_events = AgentOsPolicy::get().max_events_in_memory;
            if state.events.len() > max_events {
                let trim_count = state.events.len() - max_events;
                state.events.drain(0..trim_count);
            }
        }
        if let Some(db) = self.state_db.read().await.clone() {
            let thread_id = entry.thread_id.map(|id| id.to_string());
            if let Err(err) = db
                .record_agent_os_event_json(
                    entry.event_id.as_str(),
                    entry.created_at.timestamp(),
                    entry.event_type.as_str(),
                    thread_id.as_deref(),
                    entry.task_id.as_deref(),
                    entry.command_id.as_deref(),
                    &entry.payload,
                )
                .await
            {
                tracing::warn!("failed to persist AgentOS event: {err}");
            }
        }
        self.notify_changed();
    }

    async fn persist_thread_snapshot(&self, entry: &ThreadRegistryEntry) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(entry) else {
            return;
        };
        let thread_id = entry.thread_id.to_string();
        if let Err(err) = db
            .upsert_agent_os_thread_snapshot(thread_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS thread snapshot: {err}");
        }
    }

    async fn persist_task_snapshot(&self, task: &TaskRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(task) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_task_snapshot(task.task_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS task snapshot: {err}");
        }
    }

    async fn persist_lease_snapshot(&self, lease: &ResourceLease) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(lease) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_lease_snapshot(lease.lease_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS lease snapshot: {err}");
        }
    }

    async fn persist_ticket_snapshot(&self, ticket: &ExecutionTicket) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Issued"));
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS ticket snapshot: {err}");
        }
    }

    async fn persist_started_ticket_snapshot(&self, ticket: &ExecutionTicket, command_id: &str) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Started"));
            object.insert("command_id".to_string(), json!(command_id));
            object.insert("started_at".to_string(), json!(Utc::now().to_rfc3339()));
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist started AgentOS ticket snapshot: {err}");
        }
    }

    async fn persist_revoked_ticket_snapshot(&self, ticket: &ExecutionTicket, reason: &str) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Revoked"));
            object.insert("revoked_reason".to_string(), json!(reason));
            object.insert("revoked_at".to_string(), json!(Utc::now().to_rfc3339()));
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist revoked AgentOS ticket snapshot: {err}");
        }
    }

    async fn persist_finished_ticket_snapshot(
        &self,
        ticket: &ExecutionTicket,
        success: Option<bool>,
    ) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Finished"));
            object.insert("finished_at".to_string(), json!(Utc::now().to_rfc3339()));
            if let Some(success) = success {
                object.insert("success".to_string(), json!(success));
            }
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist finished AgentOS ticket snapshot: {err}");
        }
    }

    async fn persist_intent_plan_snapshot(&self, plan: &CommandIntentPlan) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(plan) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_intent_plan_snapshot(plan.plan_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS intent plan snapshot: {err}");
        }
    }

    async fn persist_command_snapshot(&self, command: &CommandRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(command) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_command_snapshot(command.command_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS command snapshot: {err}");
        }
    }

    async fn persist_process_snapshot(&self, process: &ManagedProcessRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(process) else {
            return;
        };
        let process_key =
            process_registry_key(process.process_id, process.runtime_owner_id.as_deref());
        if let Err(err) = db
            .upsert_agent_os_process_snapshot(process_key.as_str(), process.process_id, &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS process snapshot: {err}");
        }
    }

    async fn persist_runtime_command_snapshot(&self, command: &RuntimeCommandRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(command) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_runtime_command_snapshot(command.command_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS runtime command snapshot: {err}");
        }
    }

    async fn persist_artifact_snapshot(&self, artifact: &ArtifactRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(artifact) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_artifact_snapshot(artifact.artifact_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS artifact snapshot: {err}");
        }
    }

    async fn persist_worker_request_snapshot(&self, request: &WorkerRequestRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(request) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_worker_request_snapshot(request.request_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS worker request snapshot: {err}");
        }
    }
}

impl AgentOsState {
    fn ensure_builtin_profiles(&mut self) {
        if self.profiles.is_empty() {
            for profile in builtin_profiles() {
                self.profiles.insert(profile.profile_id.clone(), profile);
            }
        }
    }
}

impl CapabilityProfile {
    fn validate_command_intent(
        &self,
        intent: &ActionIntent,
        command: &[String],
        cwd: &Path,
    ) -> Result<(), String> {
        if self.command_denies(command) {
            return Err("command denied by AgentOS command denylist".to_string());
        }
        if !self.can_run_shell {
            return Err("profile cannot run shell commands".to_string());
        }
        if self.intent_scopes.deny.contains(&intent.kind) {
            return Err(format!("intent `{}` is denied", intent.kind.as_str()));
        }
        if !self.intent_scopes.allow.is_empty() && !self.intent_scopes.allow.contains(&intent.kind)
        {
            return Err(format!(
                "intent `{}` is outside allowed intents",
                intent.kind.as_str()
            ));
        }
        if requires_write(intent.kind) && !self.can_write_files {
            return Err("profile cannot write files".to_string());
        }
        if requires_compile(intent.kind) && !self.can_compile {
            return Err("profile cannot compile".to_string());
        }
        if requires_cpu_heavy(intent.kind) && !self.can_cpu_heavy {
            return Err("profile cannot use CPU-heavy execution".to_string());
        }
        if intent.kind == ActionIntentKind::RunApp && !self.can_run_app {
            return Err("profile cannot run app runtimes".to_string());
        }
        if intent.kind == ActionIntentKind::Gpu && !self.can_use_gpu {
            return Err("profile cannot use GPU resources".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Gpu { .. }))
            && !self.can_use_gpu
        {
            return Err("profile cannot use GPU resources".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Port { .. }))
            && !self.can_hold_ports
        {
            return Err("profile cannot hold ports".to_string());
        }
        if intent.kind == ActionIntentKind::Network && !self.can_network {
            return Err("profile cannot use network resources".to_string());
        }
        if intent.kind == ActionIntentKind::GitMutation && !self.can_modify_git {
            return Err("profile cannot modify git state".to_string());
        }
        if intent.kind == ActionIntentKind::LongProcess && !self.can_spawn_long_process {
            return Err("profile cannot spawn long-running processes".to_string());
        }
        if !self.path_scopes.allows(cwd) {
            return Err(format!(
                "cwd `{}` is outside profile path scope",
                cwd.display()
            ));
        }
        Ok(())
    }

    fn validate_tool_intent(&self, intent: &ActionIntent) -> Result<(), String> {
        if self.intent_scopes.deny.contains(&intent.kind) {
            return Err(format!("intent `{}` is denied", intent.kind.as_str()));
        }
        if !self.intent_scopes.allow.is_empty() && !self.intent_scopes.allow.contains(&intent.kind)
        {
            return Err(format!(
                "intent `{}` is outside allowed intents",
                intent.kind.as_str()
            ));
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::RepoWrite { .. }))
            && !self.can_write_files
        {
            return Err("profile cannot write files".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Network { .. }))
            && !self.can_network
        {
            return Err("profile cannot use network or external side-effect tools".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Gpu { .. }))
            && !self.can_use_gpu
        {
            return Err("profile cannot use GPU resources".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Port { .. }))
            && !self.can_hold_ports
        {
            return Err("profile cannot hold ports".to_string());
        }
        if intent
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::GitIndex { .. }))
            && !self.can_modify_git
        {
            return Err("profile cannot modify git".to_string());
        }
        Ok(())
    }

    fn command_denies(&self, command: &[String]) -> bool {
        let rendered = denylist_surface(command).to_ascii_lowercase();
        self.command_denylist
            .iter()
            .any(|pattern| rendered.contains(&pattern.to_ascii_lowercase()))
    }

    fn capability_names_for_action(&self, action: &ActionIntent) -> Vec<String> {
        let intent = action.kind;
        let mut caps = vec!["run_shell".to_string()];
        if requires_write(intent) {
            caps.push("write_files".to_string());
        }
        if requires_compile(intent) {
            caps.push("compile".to_string());
        }
        if requires_cpu_heavy(intent) {
            caps.push("cpu_heavy".to_string());
        }
        if intent == ActionIntentKind::RunApp {
            caps.push("run_app".to_string());
        }
        if intent == ActionIntentKind::Gpu {
            caps.push("gpu".to_string());
        }
        if intent == ActionIntentKind::Harness {
            caps.push("harness".to_string());
        }
        if action
            .required_resources
            .iter()
            .any(|resource| matches!(resource, ResourceRequirement::Gpu { .. }))
        {
            caps.push("gpu".to_string());
        }
        if intent == ActionIntentKind::Network {
            caps.push("network".to_string());
        }
        if intent == ActionIntentKind::GitMutation {
            caps.push("modify_git".to_string());
        }
        caps.sort();
        caps.dedup();
        caps
    }
}

impl ScopedPaths {
    fn allows(&self, path: &Path) -> bool {
        let value = normalize_path_for_scope(path);
        if self
            .deny
            .iter()
            .any(|pattern| scope_matches(pattern, &value))
        {
            return false;
        }
        self.allow.is_empty()
            || self
                .allow
                .iter()
                .any(|pattern| scope_matches(pattern, &value))
    }
}

pub(crate) fn rank_for_session_source(source: &SessionSource) -> u8 {
    match source {
        SessionSource::SubAgent(_) => 2,
        _ => COORDINATOR_RANK,
    }
}

pub(crate) fn profile_for_rank(rank: u8) -> &'static str {
    match rank {
        COORDINATOR_RANK => "coordinator",
        _ => "worker",
    }
}

pub(crate) fn coordination_scope_for_session_source(
    source: &SessionSource,
    thread_id: ThreadId,
) -> String {
    match source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        }) => format!("root:{parent_thread_id}"),
        _ => format!("root:{thread_id}"),
    }
}

pub(crate) fn classify_command(command: &[String], cwd: &Path) -> ActionIntent {
    let rendered = command.join(" ").to_ascii_lowercase();
    let repo_scope = repo_scope_for_cwd(cwd);
    let mut resources = Vec::new();
    let mut side_effects = Vec::new();
    let (kind, confidence, risk_level) = if rendered.contains("apply_patch") {
        resources.push(ResourceRequirement::RepoWrite { scope: repo_scope });
        side_effects.push("writes files".to_string());
        (ActionIntentKind::FileWrite, 0.98, "medium")
    } else if is_git_mutation(&rendered) {
        resources.push(ResourceRequirement::GitIndex {
            scope: repo_scope.clone(),
        });
        resources.push(ResourceRequirement::RepoWrite { scope: repo_scope });
        side_effects.push("mutates git state".to_string());
        (ActionIntentKind::GitMutation, 0.94, "high")
    } else if is_run_app_command(&rendered) {
        resources.push(ResourceRequirement::AppRuntime {
            scope: repo_scope.clone(),
        });
        if let Some(port) = extract_port(&rendered) {
            resources.push(ResourceRequirement::Port { port });
        }
        side_effects.push("starts long-running app runtime".to_string());
        (ActionIntentKind::RunApp, 0.91, "medium")
    } else if is_harness_command(&rendered) {
        if is_gpu_command(&rendered) {
            resources.push(ResourceRequirement::Gpu {
                scope: "default".to_string(),
            });
            side_effects.push("uses GPU harness resources".to_string());
        }
        side_effects.push("runs a prebuilt verification harness".to_string());
        (ActionIntentKind::Harness, 0.88, "medium")
    } else if is_test_command(&rendered) {
        resources.push(ResourceRequirement::CpuHeavy);
        resources.push(ResourceRequirement::BuildCache { scope: repo_scope });
        side_effects.push("writes build/test artifacts".to_string());
        (ActionIntentKind::Test, 0.92, "medium")
    } else if is_compile_command(&rendered) {
        resources.push(ResourceRequirement::CpuHeavy);
        resources.push(ResourceRequirement::BuildCache { scope: repo_scope });
        side_effects.push("writes build artifacts".to_string());
        (ActionIntentKind::Compile, 0.90, "medium")
    } else if is_network_command(&rendered) {
        resources.push(ResourceRequirement::Network {
            scope: "default".to_string(),
        });
        side_effects.push("uses network".to_string());
        (ActionIntentKind::Network, 0.86, "high")
    } else if is_file_write_command(&rendered) {
        resources.push(ResourceRequirement::RepoWrite { scope: repo_scope });
        side_effects.push("may write files".to_string());
        (ActionIntentKind::FileWrite, 0.78, "medium")
    } else if is_long_process_command(&rendered) {
        resources.push(ResourceRequirement::CpuHeavy);
        side_effects.push("may run for a long time".to_string());
        (ActionIntentKind::LongProcess, 0.72, "medium")
    } else if is_read_only_command(&rendered) {
        (ActionIntentKind::ReadOnly, 0.84, "low")
    } else {
        resources.push(ResourceRequirement::CpuHeavy);
        side_effects.push("unknown shell side effects".to_string());
        (ActionIntentKind::UnknownRisky, 0.40, "high")
    };

    ActionIntent {
        kind,
        confidence,
        required_resources: resources,
        side_effects,
        risk_level: risk_level.to_string(),
    }
}

fn classify_mutating_tool(tool_name: &str) -> ActionIntent {
    ActionIntent {
        kind: ActionIntentKind::UnknownRisky,
        confidence: 0.50,
        required_resources: vec![ResourceRequirement::Network {
            scope: "external_tool".to_string(),
        }],
        side_effects: vec![format!("mutating external tool `{tool_name}`")],
        risk_level: "high".to_string(),
    }
}

fn builtin_profiles() -> Vec<CapabilityProfile> {
    vec![
        CapabilityProfile {
            profile_id: "coordinator".to_string(),
            can_read_files: true,
            can_write_files: true,
            can_run_shell: true,
            can_cpu_heavy: true,
            can_compile: true,
            can_run_app: true,
            can_use_gpu: true,
            can_hold_ports: true,
            can_network: true,
            can_modify_git: true,
            can_spawn_long_process: true,
            path_scopes: ScopedPaths {
                allow: vec!["**".to_string()],
                deny: Vec::new(),
            },
            intent_scopes: ScopedIntents::default(),
            command_denylist: dangerous_command_denylist(),
        },
        CapabilityProfile {
            profile_id: "worker".to_string(),
            can_read_files: true,
            can_write_files: true,
            can_run_shell: true,
            can_cpu_heavy: false,
            can_compile: false,
            can_run_app: false,
            can_use_gpu: true,
            can_hold_ports: false,
            can_network: false,
            can_modify_git: false,
            can_spawn_long_process: false,
            path_scopes: ScopedPaths {
                allow: vec!["**".to_string()],
                deny: vec!["state/migrations/**".to_string(), ".github/**".to_string()],
            },
            intent_scopes: ScopedIntents {
                allow: Vec::new(),
                deny: vec![
                    ActionIntentKind::Compile,
                    ActionIntentKind::Test,
                    ActionIntentKind::RunApp,
                    ActionIntentKind::LongProcess,
                    ActionIntentKind::GitMutation,
                    ActionIntentKind::Network,
                ],
            },
            command_denylist: dangerous_command_denylist(),
        },
    ]
}

fn dangerous_command_denylist() -> Vec<String> {
    vec![
        "rm -rf /".to_string(),
        "curl | sh".to_string(),
        "wget | sh".to_string(),
        "sudo ".to_string(),
        "chmod -r 777".to_string(),
        "git reset --hard".to_string(),
    ]
}

fn denylist_surface(command: &[String]) -> String {
    if command
        .first()
        .is_some_and(|program| program.eq_ignore_ascii_case("apply_patch"))
    {
        return "apply_patch".to_string();
    }
    command.join(" ")
}

fn runtime_kind_for_intent(intent: ActionIntentKind) -> &'static str {
    match intent {
        ActionIntentKind::RunApp | ActionIntentKind::LongProcess => {
            process_runtime_kind::LONG_PROCESS
        }
        ActionIntentKind::Compile | ActionIntentKind::Test | ActionIntentKind::Harness => {
            process_runtime_kind::COMMAND
        }
        ActionIntentKind::Gpu => process_runtime_kind::GPU_COMMAND,
        ActionIntentKind::Network => process_runtime_kind::NETWORK_COMMAND,
        _ => process_runtime_kind::COMMAND,
    }
}

fn artifact_type_for_intent(intent: ActionIntentKind) -> ArtifactType {
    match intent {
        ActionIntentKind::Compile | ActionIntentKind::Test | ActionIntentKind::Harness => {
            ArtifactType::CompileLog
        }
        _ => ArtifactType::CommandLog,
    }
}

fn summarize_output(raw_output: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw_output);
    let mut summary = text.lines().take(20).collect::<Vec<_>>().join("\n");
    truncate_to_char_boundary(&mut summary, 2_000);
    summary
}

fn requires_write(intent: ActionIntentKind) -> bool {
    matches!(
        intent,
        ActionIntentKind::FileWrite
            | ActionIntentKind::Compile
            | ActionIntentKind::Test
            | ActionIntentKind::RunApp
            | ActionIntentKind::GitMutation
            | ActionIntentKind::UnknownRisky
    )
}

fn requires_dirty_audit(intent: ActionIntentKind) -> bool {
    requires_write(intent) || matches!(intent, ActionIntentKind::GitMutation)
}

fn requires_compile(intent: ActionIntentKind) -> bool {
    matches!(intent, ActionIntentKind::Compile | ActionIntentKind::Test)
}

fn requires_cpu_heavy(intent: ActionIntentKind) -> bool {
    matches!(
        intent,
        ActionIntentKind::Compile
            | ActionIntentKind::Test
            | ActionIntentKind::LongProcess
            | ActionIntentKind::UnknownRisky
    )
}

fn validate_task_action_contract(
    task: &TaskRecord,
    required_capabilities: &[String],
    required_resources: &[ResourceRequirement],
) -> PraxisResult<()> {
    if !task.required_capabilities.is_empty() {
        let declared = task
            .required_capabilities
            .iter()
            .map(|capability| capability.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        if let Some(missing) = required_capabilities
            .iter()
            .find(|capability| !declared.contains(&capability.to_ascii_lowercase()))
        {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "action rejected: required capability `{missing}` is outside task capability contract"
            )));
        }
    }
    if !task.required_resources.is_empty() {
        for resource in required_resources {
            if !task
                .required_resources
                .iter()
                .any(|declared| task_resource_allows(declared, resource))
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "action rejected: required resource `{}` is outside task resource contract",
                    resource.key()
                )));
            }
        }
    }
    Ok(())
}

fn task_resource_allows(declared: &ResourceRequirement, required: &ResourceRequirement) -> bool {
    if declared.key() == required.key() {
        return true;
    }
    match (declared, required) {
        (ResourceRequirement::CpuHeavy, ResourceRequirement::CpuHeavy) => true,
        (ResourceRequirement::Port { port: declared }, ResourceRequirement::Port { port }) => {
            declared == port
        }
        (ResourceRequirement::Gpu { scope: declared }, ResourceRequirement::Gpu { scope }) => {
            scoped_resource_allows(declared, scope, true)
        }
        (
            ResourceRequirement::Network { scope: declared },
            ResourceRequirement::Network { scope },
        ) => scoped_resource_allows(declared, scope, true),
        (
            ResourceRequirement::LlmBudget { scope: declared },
            ResourceRequirement::LlmBudget { scope },
        ) => scoped_resource_allows(declared, scope, true),
        (
            ResourceRequirement::BuildCache { scope: declared },
            ResourceRequirement::BuildCache { scope },
        ) => scoped_resource_allows(declared, scope, false),
        (
            ResourceRequirement::AppRuntime { scope: declared },
            ResourceRequirement::AppRuntime { scope },
        ) => scoped_resource_allows(declared, scope, false),
        (
            ResourceRequirement::GitIndex { scope: declared },
            ResourceRequirement::GitIndex { scope },
        ) => scoped_resource_allows(declared, scope, false),
        (
            ResourceRequirement::RepoWrite { scope: declared },
            ResourceRequirement::RepoWrite { scope },
        ) => repo_write_resource_allows(declared, scope),
        _ => false,
    }
}

fn scoped_resource_allows(declared: &str, required: &str, allow_default: bool) -> bool {
    let declared = normalize_resource_scope(declared);
    let required = normalize_resource_scope(required);
    declared == required
        || declared == "*"
        || declared == "**"
        || (allow_default && declared == "default")
        || (declared.contains('*') && wildcard_match(declared.as_str(), required.as_str()))
}

fn repo_write_resource_allows(declared: &str, required: &str) -> bool {
    let declared = normalize_resource_scope(declared);
    let required = normalize_resource_scope(required);
    if declared == required || declared == "*" || declared == "**" {
        return true;
    }
    if declared.starts_with("repo:") {
        return scoped_resource_allows(declared.as_str(), required.as_str(), false);
    }
    // A path-scoped repo_write contract cannot know the exact touched files before
    // a shell command runs. Allow the command to start only under dirty-file audit;
    // actual files are checked against Task.scope/CapabilityProfile.path_scopes
    // after execution and policy-violating tasks are failed. This is intentionally
    // narrower than the old same-resource-type fallback.
    required.starts_with("repo:")
}

fn normalize_resource_scope(scope: &str) -> String {
    scope.trim().replace('\\', "/").to_ascii_lowercase()
}

fn capacity_for_requirement(requirement: &ResourceRequirement) -> usize {
    match requirement {
        ResourceRequirement::CpuHeavy => 1,
        ResourceRequirement::LlmBudget { .. } => 8,
        _ => 1,
    }
}

fn dirty_file_allowed_by_task(task: &TaskRecord, path: &Path) -> bool {
    if task.exploratory || task.scope.is_empty() {
        return true;
    }
    let value = normalize_path_for_scope(path);
    task.scope
        .iter()
        .any(|pattern| scope_matches(pattern, &value))
}

fn dirty_file_delta(
    cwd: &Path,
    before: &[PathBuf],
    before_fingerprints: &HashMap<String, DirtyFileFingerprint>,
    after: &[PathBuf],
) -> Vec<PathBuf> {
    let before = before
        .iter()
        .map(|path| normalize_path_for_scope(path))
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    after
        .iter()
        .filter_map(|path| {
            let normalized = normalize_path_for_scope(path);
            if !seen.insert(normalized.clone()) {
                return None;
            }
            if before.contains(&normalized) {
                let current = dirty_file_fingerprint(cwd, path);
                if before_fingerprints.get(&normalized) == Some(&current) {
                    return None;
                }
            }
            Some(path.clone())
        })
        .collect()
}

fn push_unique_dirty_files(target: &mut Vec<PathBuf>, dirty_files: &[PathBuf]) {
    let mut seen = target
        .iter()
        .map(|path| normalize_path_for_scope(path))
        .collect::<HashSet<_>>();
    for path in dirty_files {
        if seen.insert(normalize_path_for_scope(path)) {
            target.push(path.clone());
        }
    }
}

async fn audit_git_dirty_files(cwd: &Path) -> Vec<PathBuf> {
    let repo_root = find_repo_root(cwd);
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .output()
        .await;
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_git_status_porcelain_z(&output.stdout)
        .into_iter()
        .map(|path| {
            if path.is_absolute() {
                path
            } else if let Some(root) = repo_root.as_ref() {
                root.join(path)
            } else {
                cwd.join(path)
            }
        })
        .collect()
}

fn dirty_file_fingerprints(
    cwd: &Path,
    dirty_files: &[PathBuf],
) -> HashMap<String, DirtyFileFingerprint> {
    dirty_files
        .iter()
        .map(|path| {
            (
                normalize_path_for_scope(path),
                dirty_file_fingerprint(cwd, path),
            )
        })
        .collect()
}

fn dirty_file_fingerprint(cwd: &Path, path: &Path) -> DirtyFileFingerprint {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let Ok(metadata) = std::fs::metadata(path) else {
        return DirtyFileFingerprint {
            exists: false,
            len: None,
            modified_unix_millis: None,
        };
    };
    let modified_unix_millis = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i128);
    DirtyFileFingerprint {
        exists: true,
        len: Some(metadata.len()),
        modified_unix_millis,
    }
}

fn parse_git_status_porcelain_z(output: &[u8]) -> Vec<PathBuf> {
    let entries = output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    let mut paths = Vec::new();
    let mut idx = 0;
    while idx < entries.len() {
        let entry = entries[idx];
        if entry.len() < 4 || entry[2] != b' ' {
            idx += 1;
            continue;
        }
        let status = entry[0];
        let path = String::from_utf8_lossy(&entry[3..]).to_string();
        if !path.is_empty() {
            paths.push(PathBuf::from(path));
        }
        idx += if matches!(status, b'R' | b'C') { 2 } else { 1 };
    }
    paths
}

fn format_dirty_file_report(dirty_files: &[PathBuf], violation_path: Option<&PathBuf>) -> String {
    let mut report = if dirty_files.is_empty() {
        "No dirty files detected.".to_string()
    } else {
        dirty_files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    if let Some(path) = violation_path {
        report.push_str("\n\nPolicy violation: dirty file outside task/profile scope: ");
        report.push_str(path.display().to_string().as_str());
    }
    report
}

fn sanitize_artifact_extension(extension: &str) -> String {
    let extension = extension
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    if extension.is_empty() {
        "bin".to_string()
    } else {
        extension
    }
}

fn metadata_with_blob(
    metadata: serde_json::Value,
    blob_bytes: usize,
    blob_path: Option<&PathBuf>,
) -> serde_json::Value {
    let blob_metadata = json!({
        "blob_bytes": blob_bytes,
        "blob_path": blob_path.map(|path| path.display().to_string()),
        "blob_persisted": blob_path.is_some(),
    });
    match metadata {
        serde_json::Value::Object(mut object) => {
            object.insert("blob".to_string(), blob_metadata);
            serde_json::Value::Object(object)
        }
        value => json!({
            "metadata": value,
            "blob": blob_metadata,
        }),
    }
}

async fn append_spool_stream(
    out: &mut tokio::fs::File,
    stream: &ExecStreamSpool,
) -> std::io::Result<()> {
    let mut input = tokio::fs::File::open(stream.path.as_path()).await?;
    tokio::io::copy(&mut input, out).await?;
    Ok(())
}

fn action_fingerprint(command: &[String], cwd: &Path, intent: ActionIntentKind) -> String {
    let mut hasher = DefaultHasher::new();
    intent.hash(&mut hasher);
    normalize_path_for_scope(&stable_path(cwd)).hash(&mut hasher);
    for arg in command {
        arg.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn repo_scope_for_cwd(cwd: &Path) -> String {
    let root = find_repo_root(cwd).unwrap_or_else(|| stable_path(cwd));
    let normalized = normalize_path_for_scope(&root);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo");
    format!("repo:{name}:{:016x}", hasher.finish())
}

fn stable_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn find_repo_root(cwd: &Path) -> Option<PathBuf> {
    let mut current = if cwd.is_file() {
        cwd.parent()?.to_path_buf()
    } else {
        cwd.to_path_buf()
    };
    loop {
        if current.join(".git").exists() {
            return Some(stable_path(&current));
        }
        if !current.pop() {
            return None;
        }
    }
}

fn is_test_command(command: &str) -> bool {
    command.contains(" test")
        || command.contains("cargo nextest")
        || command.contains("pytest")
        || command.contains("vitest")
        || command.contains("jest")
        || command.contains("go test")
}

fn is_harness_command(command: &str) -> bool {
    [
        "harness",
        "native_harness",
        "parity_harness",
        "compare_harness",
        "target/debug/",
        "target\\debug\\",
        "target/release/",
        "target\\release\\",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_gpu_command(command: &str) -> bool {
    [
        "gpu", "cuda", "nvidia", "vulkan", "wgpu", "directx", "d3d12", "metal",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_compile_command(command: &str) -> bool {
    [
        "cargo build",
        "cargo check",
        "cargo run",
        "npm run build",
        "pnpm build",
        "pnpm turbo build",
        "yarn build",
        "just build",
        "ninja",
        "bazel build",
        "make",
        "cmake --build",
        "maturin",
        "python setup.py build",
        "dotnet build",
        "msbuild",
        "gradle build",
        "mvn package",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_run_app_command(command: &str) -> bool {
    [
        "npm run dev",
        "pnpm dev",
        "yarn dev",
        "vite",
        "next dev",
        "cargo run",
        "trunk serve",
        "python -m http.server",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_network_command(command: &str) -> bool {
    [
        "curl ",
        "wget ",
        "git clone",
        "npm install",
        "pnpm install",
        "yarn install",
        "cargo fetch",
        "pip install",
        "uv pip install",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_file_write_command(command: &str) -> bool {
    [
        "apply_patch",
        "set-content",
        "out-file",
        "new-item",
        "remove-item",
        "move-item",
        "copy-item",
        "python -c",
        "node -e",
        "tee ",
        ">",
        ">>",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_git_mutation(command: &str) -> bool {
    [
        "git commit",
        "git rebase",
        "git merge",
        "git checkout",
        "git switch",
        "git reset",
        "git clean",
        "git stash",
        "git add",
        "git rm",
        "git mv",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_long_process_command(command: &str) -> bool {
    [
        "watch ",
        "tail -f",
        "sleep ",
        "python train.py",
        "tensorboard",
        "jupyter",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn is_read_only_command(command: &str) -> bool {
    [
        "rg ",
        "grep ",
        "get-content",
        "select-string",
        "ls",
        "dir",
        "git status",
        "git diff",
        "git show",
        "git log",
        "findstr",
        "type ",
        "cat ",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn extract_port(command: &str) -> Option<u16> {
    for marker in ["--port ", "-p "] {
        if let Some((_, suffix)) = command.split_once(marker) {
            let digits: String = suffix
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            if let Ok(port) = digits.parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_matches_path_segments_not_substrings() {
        assert!(scope_matches("tui/src/**", "/repo/praxis/tui/src/app.rs"));
        assert!(scope_matches("tui/src", "/repo/praxis/tui/src/app.rs"));
        assert!(scope_matches("/repo/praxis", "/repo/praxis/tui/src/app.rs"));
        assert!(scope_matches("*.rs", "/repo/praxis/tui/src/app.rs"));
        assert!(!scope_matches("app", "/repo/praxis/myapp2/src/main.rs"));
        assert!(!scope_matches(
            "tui/src/**",
            "/repo/praxis/tui/src_backup/app.rs"
        ));
        assert!(!scope_matches(
            "state/migrations/**",
            "/repo/praxis/tui/src/app.rs"
        ));
    }

    #[test]
    fn task_resource_allows_never_falls_back_to_same_type_only() {
        assert!(!task_resource_allows(
            &ResourceRequirement::BuildCache {
                scope: "repo:a".to_string()
            },
            &ResourceRequirement::BuildCache {
                scope: "repo:b".to_string()
            },
        ));
        assert!(!task_resource_allows(
            &ResourceRequirement::GitIndex {
                scope: "worktree:a".to_string()
            },
            &ResourceRequirement::GitIndex {
                scope: "worktree:b".to_string()
            },
        ));
        assert!(task_resource_allows(
            &ResourceRequirement::Network {
                scope: "default".to_string()
            },
            &ResourceRequirement::Network {
                scope: "external_tool".to_string()
            },
        ));
    }
}

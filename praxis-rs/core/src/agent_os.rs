use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use async_trait::async_trait;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use praxis_rollout::StateDbHandle;

const COORDINATOR_RANK: u8 = 0;
const MAX_COORDINATORS: usize = 3;
const DEFAULT_TICKET_TTL_SECONDS: i64 = 30 * 60;
const DEFAULT_LEASE_TTL_SECONDS: i64 = 30 * 60;

#[async_trait]
pub(crate) trait AgentOsProcessCleaner: Send + Sync {
    async fn cleanup_agent_os_process(&self, process_id: i32) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ThreadRuntimeState {
    Idle,
    Assigned,
    Running,
    WaitingForLease,
    WaitingForCoordinator,
    Paused,
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
    fn key(&self) -> String {
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
    pub(crate) command_allowlist: Vec<String>,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ExecutionTicket {
    pub(crate) ticket_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) coordination_scope: String,
    pub(crate) allowed_intent: ActionIntentKind,
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
pub(crate) struct CommandRecord {
    pub(crate) command_id: String,
    pub(crate) ticket_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) intent: ActionIntentKind,
    pub(crate) command_fingerprint: String,
    pub(crate) raw_command: String,
    pub(crate) cwd: PathBuf,
    pub(crate) process_id: Option<i32>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) ended_at: Option<DateTime<Utc>>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) lease_ids: Vec<String>,
    pub(crate) artifacts: Vec<String>,
    pub(crate) dirty_files: Vec<PathBuf>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ArtifactType {
    CommandLog,
    CompileLog,
    DirtyFileReport,
    DecisionRecord,
    PatchMetadata,
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
    fn as_str(self) -> &'static str {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RuntimeCommand {
    pub(crate) command_id: String,
    pub(crate) from_thread_id: ThreadId,
    pub(crate) to_thread_id: ThreadId,
    pub(crate) coordinator_epoch: u64,
    pub(crate) command_type: RuntimeCommandType,
    pub(crate) payload: serde_json::Value,
    pub(crate) status: RuntimeCommandStatus,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ActiveCoordinatorLease {
    coordination_scope: String,
    owner_thread_id: ThreadId,
    epoch: u64,
    fencing_token: u64,
    expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ActiveCoordinatorStatus {
    pub(crate) coordination_scope: String,
    pub(crate) owner_thread_id: ThreadId,
    pub(crate) epoch: u64,
    pub(crate) fencing_token: u64,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Default)]
struct AgentOsState {
    threads: HashMap<ThreadId, ThreadRegistryEntry>,
    profiles: HashMap<String, CapabilityProfile>,
    tasks: HashMap<String, TaskRecord>,
    leases: HashMap<String, ResourceLease>,
    tickets: HashMap<String, ExecutionTicket>,
    commands: HashMap<String, CommandRecord>,
    runtime_commands: HashMap<String, RuntimeCommand>,
    artifacts: HashMap<String, ArtifactRecord>,
    events: Vec<EventLedgerEntry>,
    active_coordinators: HashMap<String, ActiveCoordinatorLease>,
    fencing_counter: u64,
    coordinator_epoch: u64,
}

#[derive(Default)]
pub(crate) struct AgentOsRuntime {
    state: RwLock<AgentOsState>,
    state_db: RwLock<Option<StateDbHandle>>,
    process_cleaner: RwLock<Option<Arc<dyn AgentOsProcessCleaner>>>,
}

impl AgentOsRuntime {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) async fn attach_state_db(&self, state_db: Option<StateDbHandle>) {
        if let Some(state_db) = state_db {
            *self.state_db.write().await = Some(state_db);
        }
    }

    pub(crate) async fn attach_process_cleaner<T>(&self, process_cleaner: Arc<T>)
    where
        T: AgentOsProcessCleaner + 'static,
    {
        let process_cleaner: Arc<dyn AgentOsProcessCleaner> = process_cleaner;
        *self.process_cleaner.write().await = Some(process_cleaner);
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
                    .count();
                if coordinator_count >= MAX_COORDINATORS {
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "rank-0 coordinator limit reached for scope `{}`",
                        entry.coordination_scope
                    )));
                }
                if !state
                    .active_coordinators
                    .contains_key(entry.coordination_scope.as_str())
                {
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
                            expires_at: now + Duration::seconds(DEFAULT_LEASE_TTL_SECONDS),
                        },
                    );
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

    pub(crate) async fn issue_runtime_command(
        &self,
        from_thread_id: ThreadId,
        to_thread_id: ThreadId,
        command_type: RuntimeCommandType,
        payload: serde_json::Value,
    ) -> PraxisResult<String> {
        let now = Utc::now();
        let command_id = format!("runtime-cmd-{}", Uuid::new_v4());
        let (command, task_id, coordination_scope) = {
            let mut state = self.state.write().await;
            let sender = state.threads.get(&from_thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS sender thread `{from_thread_id}`"
                ))
            })?;
            if sender.rank != COORDINATOR_RANK {
                return Err(PraxisErr::UnsupportedOperation(
                    "only rank-0 coordinator threads can issue runtime commands".to_string(),
                ));
            }
            let coordination_scope = sender.coordination_scope.clone();
            let coordinator_epoch = {
                let active = state
                    .active_coordinators
                    .get_mut(coordination_scope.as_str())
                    .ok_or_else(|| {
                        PraxisErr::UnsupportedOperation("no active coordinator lease".to_string())
                    })?;
                if active.expires_at <= now {
                    if active.owner_thread_id != from_thread_id {
                        return Err(PraxisErr::UnsupportedOperation(
                            "active coordinator lease has expired".to_string(),
                        ));
                    }
                    active.expires_at = now + Duration::seconds(DEFAULT_LEASE_TTL_SECONDS);
                }
                if active.owner_thread_id != from_thread_id {
                    return Err(PraxisErr::UnsupportedOperation(
                        "only the active rank-0 coordinator can dispatch runtime commands"
                            .to_string(),
                    ));
                }
                active.epoch
            };
            let receiver = state.threads.get(&to_thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS receiver thread `{to_thread_id}`"
                ))
            })?;
            if receiver.coordination_scope != coordination_scope {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime commands cannot cross coordination scopes".to_string(),
                ));
            }
            let command = RuntimeCommand {
                command_id: command_id.clone(),
                from_thread_id,
                to_thread_id,
                coordinator_epoch,
                command_type,
                payload,
                status: RuntimeCommandStatus::Pending,
                created_at: now,
                expires_at: now + Duration::seconds(DEFAULT_TICKET_TTL_SECONDS),
            };
            let task_id = receiver.current_task_id.clone();
            state
                .runtime_commands
                .insert(command.command_id.clone(), command.clone());
            (command, task_id, coordination_scope)
        };

        self.persist_runtime_command_snapshot(&command).await;
        self.record_event(
            "runtime_command_issued",
            Some(to_thread_id),
            task_id,
            Some(command_id.clone()),
            json!({
                "from_thread_id": from_thread_id.to_string(),
                "to_thread_id": to_thread_id.to_string(),
                "coordination_scope": coordination_scope,
                "command_type": command.command_type.as_str(),
                "coordinator_epoch": command.coordinator_epoch,
                "payload": command.payload,
            }),
        )
        .await;
        Ok(command_id)
    }

    pub(crate) async fn renew_active_coordinator(&self, thread_id: ThreadId) -> PraxisResult<()> {
        let lease = {
            let mut state = self.state.write().await;
            let thread = state.threads.get(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS coordinator thread `{thread_id}`"
                ))
            })?;
            let coordination_scope = thread.coordination_scope.clone();
            let active = state
                .active_coordinators
                .get_mut(coordination_scope.as_str())
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation("no active coordinator lease".to_string())
                })?;
            if active.owner_thread_id != thread_id {
                return Err(PraxisErr::UnsupportedOperation(
                    "only the active coordinator can renew its lease".to_string(),
                ));
            }
            active.expires_at = Utc::now() + Duration::seconds(DEFAULT_LEASE_TTL_SECONDS);
            active.clone()
        };
        self.record_event(
            "active_coordinator_renewed",
            Some(thread_id),
            None,
            None,
            json!({
                "coordination_scope": lease.coordination_scope,
                "epoch": lease.epoch,
                "fencing_token": lease.fencing_token,
                "expires_at": lease.expires_at,
            }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn takeover_active_coordinator(
        &self,
        thread_id: ThreadId,
    ) -> PraxisResult<()> {
        let now = Utc::now();
        let lease = {
            let mut state = self.state.write().await;
            let thread = state.threads.get(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS coordinator thread `{thread_id}`"
                ))
            })?;
            if thread.rank != COORDINATOR_RANK {
                return Err(PraxisErr::UnsupportedOperation(
                    "only rank-0 threads can take over active coordinator lease".to_string(),
                ));
            }
            let coordination_scope = thread.coordination_scope.clone();
            if let Some(active) = state.active_coordinators.get(coordination_scope.as_str())
                && active.expires_at > now
                && active.owner_thread_id != thread_id
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "active coordinator lease is still live".to_string(),
                ));
            }
            state.coordinator_epoch = state.coordinator_epoch.saturating_add(1);
            state.fencing_counter = state.fencing_counter.saturating_add(1);
            let lease = ActiveCoordinatorLease {
                coordination_scope: coordination_scope.clone(),
                owner_thread_id: thread_id,
                epoch: state.coordinator_epoch,
                fencing_token: state.fencing_counter,
                expires_at: now + Duration::seconds(DEFAULT_LEASE_TTL_SECONDS),
            };
            state
                .active_coordinators
                .insert(coordination_scope, lease.clone());
            lease
        };
        self.record_event(
            "active_coordinator_takeover",
            Some(thread_id),
            None,
            None,
            json!({
                "coordination_scope": lease.coordination_scope,
                "epoch": lease.epoch,
                "fencing_token": lease.fencing_token,
                "expires_at": lease.expires_at,
            }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn update_runtime_command_status(
        &self,
        command_id: &str,
        status: RuntimeCommandStatus,
    ) -> PraxisResult<()> {
        let command = {
            let mut state = self.state.write().await;
            let command = state.runtime_commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown runtime command `{command_id}`"))
            })?;
            command.status = status;
            command.clone()
        };
        self.persist_runtime_command_snapshot(&command).await;
        self.record_event(
            "runtime_command_status_changed",
            Some(command.to_thread_id),
            None,
            Some(command.command_id.clone()),
            json!({
                "status": format!("{:?}", command.status),
                "from_thread_id": command.from_thread_id.to_string(),
            }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn ack_pending_runtime_commands(
        &self,
        thread_id: ThreadId,
    ) -> PraxisResult<Vec<RuntimeCommand>> {
        let now = Utc::now();
        let (changed_commands, acked_commands) = {
            let mut state = self.state.write().await;
            let thread = state.threads.get(&thread_id).cloned().ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS receiver thread `{thread_id}`"
                ))
            })?;
            let active = state
                .active_coordinators
                .get(thread.coordination_scope.as_str())
                .cloned();
            let mut changed_commands = Vec::new();
            let mut acked_commands = Vec::new();
            for command in state.runtime_commands.values_mut().filter(|command| {
                command.to_thread_id == thread_id && command.status == RuntimeCommandStatus::Pending
            }) {
                if command.expires_at <= now {
                    command.status = RuntimeCommandStatus::Expired;
                    changed_commands.push(command.clone());
                    continue;
                }
                if let Some(active) = active.as_ref()
                    && command.coordinator_epoch != active.epoch
                {
                    command.status = RuntimeCommandStatus::Rejected;
                    changed_commands.push(command.clone());
                    continue;
                }
                command.status = RuntimeCommandStatus::Acked;
                changed_commands.push(command.clone());
                acked_commands.push(command.clone());
            }
            (changed_commands, acked_commands)
        };
        for command in &changed_commands {
            self.persist_runtime_command_snapshot(command).await;
            self.record_event(
                "runtime_command_status_changed",
                Some(command.to_thread_id),
                None,
                Some(command.command_id.clone()),
                json!({
                    "status": format!("{:?}", command.status),
                    "from_thread_id": command.from_thread_id.to_string(),
                }),
            )
            .await;
        }
        Ok(acked_commands)
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
            if active.expires_at <= Utc::now() && active.owner_thread_id != from_thread_id {
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

    pub(crate) async fn query_registry(&self) -> Vec<ThreadRegistryEntry> {
        self.state.read().await.threads.values().cloned().collect()
    }

    pub(crate) async fn query_leases(&self) -> Vec<ResourceLease> {
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

    pub(crate) async fn query_active_coordinators(&self) -> Vec<ActiveCoordinatorStatus> {
        self.state
            .read()
            .await
            .active_coordinators
            .values()
            .map(|lease| ActiveCoordinatorStatus {
                coordination_scope: lease.coordination_scope.clone(),
                owner_thread_id: lease.owner_thread_id,
                epoch: lease.epoch,
                fencing_token: lease.fencing_token,
                expires_at: lease.expires_at,
            })
            .collect()
    }

    pub(crate) async fn pause_thread(&self, thread_id: ThreadId) {
        self.mark_thread_state(thread_id, ThreadRuntimeState::Paused)
            .await;
    }

    pub(crate) async fn resume_thread(&self, thread_id: ThreadId) {
        self.mark_thread_state(thread_id, ThreadRuntimeState::Idle)
            .await;
    }

    pub(crate) async fn yield_lease(&self, lease_id: &str) {
        self.release_leases(&[lease_id.to_string()]).await;
    }

    pub(crate) async fn renew_lease(
        &self,
        lease_id: &str,
        owner_thread_id: ThreadId,
    ) -> PraxisResult<()> {
        let lease = {
            let mut state = self.state.write().await;
            let lease = state.leases.get_mut(lease_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown lease `{lease_id}`"))
            })?;
            if lease.owner_thread_id != owner_thread_id {
                return Err(PraxisErr::UnsupportedOperation(
                    "lease can only be renewed by its owner".to_string(),
                ));
            }
            lease.expires_at = Some(Utc::now() + Duration::seconds(DEFAULT_LEASE_TTL_SECONDS));
            lease.clone()
        };
        self.persist_lease_snapshot(&lease).await;
        self.record_event(
            "lease_renewed",
            Some(owner_thread_id),
            Some(lease.task_id.clone()),
            None,
            json!({
                "lease_id": lease.lease_id,
                "resource_type": lease.resource_type,
                "scope": lease.scope,
                "expires_at": lease.expires_at,
            }),
        )
        .await;
        Ok(())
    }

    pub(crate) async fn cancel_command(&self, command_id: &str) -> PraxisResult<()> {
        let _ = self
            .finish_managed_command(
                command_id,
                Some(-1),
                b"command cancelled by AgentOS",
                /*release_leases*/ true,
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn request_command_ticket(
        &self,
        thread_id: ThreadId,
        command: &[String],
        cwd: &Path,
    ) -> PraxisResult<ExecutionTicket> {
        self.expire_leases().await;
        let intent = classify_command(command, cwd);
        let now = Utc::now();
        let (thread, task, profile, coordinator_epoch, coordinator_fencing) = {
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
            (
                thread,
                task,
                profile,
                active.as_ref().map(|value| value.epoch).unwrap_or(0),
                active
                    .as_ref()
                    .map(|value| value.fencing_token)
                    .unwrap_or(0),
            )
        };

        profile
            .validate_command_intent(&intent, command, cwd)
            .map_err(PraxisErr::UnsupportedOperation)?;

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
            command_fingerprint: action_fingerprint(command, cwd, intent.kind),
            cwd: cwd.to_path_buf(),
            risk_level: intent.risk_level.clone(),
            capabilities: profile.capability_names_for_action(&intent),
            lease_ids,
            file_scopes: profile.path_scopes.allow.clone(),
            token_budget: task.token_budget,
            expires_at: now + Duration::seconds(DEFAULT_TICKET_TTL_SECONDS),
            fencing_token: coordinator_fencing,
            coordinator_epoch,
            created_at: now,
        };

        {
            let mut state = self.state.write().await;
            state
                .tickets
                .insert(ticket.ticket_id.clone(), ticket.clone());
        }
        self.persist_ticket_snapshot(&ticket).await;
        self.record_event(
            "ticket_issued",
            Some(thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent": ticket.allowed_intent.as_str(),
                "leases": &ticket.lease_ids,
            }),
        )
        .await;
        Ok(ticket)
    }

    pub(crate) async fn begin_managed_command(
        &self,
        ticket: &ExecutionTicket,
        command: String,
        argv: &[String],
        cwd: PathBuf,
        process_id: Option<i32>,
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
        let record = CommandRecord {
            command_id: command_id.clone(),
            ticket_id: ticket.ticket_id.clone(),
            task_id: ticket.task_id.clone(),
            thread_id: ticket.thread_id,
            intent: ticket.allowed_intent,
            command_fingerprint,
            raw_command: command,
            cwd,
            process_id,
            started_at: now,
            ended_at: None,
            exit_code: None,
            lease_ids: ticket.lease_ids.clone(),
            artifacts: Vec::new(),
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
                    lease_snapshots.push(lease.clone());
                }
            }
            state.commands.insert(command_id.clone(), record.clone());
            lease_snapshots
        };

        for lease in lease_snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
        self.persist_command_snapshot(&record).await;
        self.record_event(
            "command_started",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            Some(command_id.clone()),
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
        Ok(command_id)
    }

    pub(crate) async fn finish_managed_command(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        raw_output: &[u8],
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        let now = Utc::now();
        let (mut command, thread_snapshot, task_snapshot, lease_ids) = {
            let mut state = self.state.write().await;
            let command_snapshot = {
                let command = state.commands.get_mut(command_id).ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
                })?;
                command.ended_at = Some(now);
                command.exit_code = exit_code;
                command.clone()
            };
            let lease_ids = command_snapshot.lease_ids.clone();
            let thread_snapshot =
                if let Some(thread) = state.threads.get_mut(&command_snapshot.thread_id) {
                    if thread.current_command_id.as_deref() == Some(command_id) {
                        thread.current_command_id = None;
                    }
                    thread.state = ThreadRuntimeState::Idle;
                    thread.heartbeat_at = now;
                    Some(thread.clone())
                } else {
                    None
                };
            let task_snapshot = if let Some(task) = state.tasks.get_mut(&command_snapshot.task_id) {
                task.status = TaskStatus::Assigned;
                task.updated_at = now;
                Some(task.clone())
            } else {
                None
            };
            (command_snapshot, thread_snapshot, task_snapshot, lease_ids)
        };

        let artifact_id = if raw_output.is_empty() {
            None
        } else {
            Some(
                self.create_artifact(
                    command.task_id.clone(),
                    command.thread_id,
                    artifact_type_for_intent(command.intent),
                    format!("artifact://command-log/{command_id}"),
                    summarize_output(raw_output),
                    json!({
                        "command_id": command_id,
                        "bytes": raw_output.len(),
                        "exit_code": exit_code,
                    }),
                )
                .await?,
            )
        };

        if let Some(artifact_id) = artifact_id.clone() {
            command.artifacts.push(artifact_id.clone());
            let mut state = self.state.write().await;
            state
                .commands
                .insert(command_id.to_string(), command.clone());
        }

        if release_leases {
            self.release_leases(&lease_ids).await;
        }
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if let Some(task) = task_snapshot {
            self.persist_task_snapshot(&task).await;
        }
        self.persist_command_snapshot(&command).await;
        self.record_event(
            "command_finished",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "exit_code": exit_code,
                "artifact_id": artifact_id,
                "leases_released": release_leases,
            }),
        )
        .await;
        Ok(artifact_id)
    }

    pub(crate) async fn checkpoint_managed_command(
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
            .create_artifact(
                command.task_id.clone(),
                command.thread_id,
                artifact_type_for_intent(command.intent),
                format!(
                    "artifact://command-checkpoint/{command_id}/{}",
                    Uuid::new_v4()
                ),
                summarize_output(raw_output),
                json!({
                    "command_id": command_id,
                    "bytes": raw_output.len(),
                    "checkpoint": true,
                }),
            )
            .await?;
        let command_snapshot = {
            let mut state = self.state.write().await;
            if let Some(command) = state.commands.get_mut(command_id) {
                command.artifacts.push(artifact_id.clone());
                Some(command.clone())
            } else {
                None
            }
        };
        if let Some(command) = command_snapshot {
            self.persist_command_snapshot(&command).await;
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
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self.command_id_for_process(process_id).await else {
            return Ok(None);
        };
        self.checkpoint_managed_command(command_id.as_str(), raw_output)
            .await
    }

    pub(crate) async fn finish_managed_process(
        &self,
        process_id: i32,
        exit_code: Option<i32>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self.command_id_for_process(process_id).await else {
            return Ok(None);
        };
        self.finish_managed_command(command_id.as_str(), exit_code, raw_output, true)
            .await
    }

    pub(crate) async fn record_command_dirty_files(
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
            dirty_files
                .iter()
                .find(|path| !dirty_file_allowed_by_task(task, path))
                .map(|path| (command.clone(), task.clone(), path.clone()))
        };
        if let Some((command, task, path)) = violation {
            self.record_event(
                "policy_violation",
                Some(command.thread_id),
                Some(command.task_id.clone()),
                Some(command.command_id.clone()),
                json!({
                    "reason": "dirty_file_outside_task_scope",
                    "path": path.display().to_string(),
                    "task_scope": task.scope,
                }),
            )
            .await;
            return Err(PraxisErr::UnsupportedOperation(format!(
                "dirty file `{}` is outside AgentOS task scope",
                path.display()
            )));
        }
        let command = {
            let mut state = self.state.write().await;
            let command = state.commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            for path in dirty_files {
                if !command.dirty_files.contains(&path) {
                    command.dirty_files.push(path);
                }
            }
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
                    expires_at: Some(now + Duration::seconds(DEFAULT_LEASE_TTL_SECONDS)),
                    revocable: true,
                    metadata: json!({}),
                    command_id: None,
                    process_id: None,
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
                cleanup_processes.insert(process_id);
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
                    "requires_process_cleanup": lease.process_id.is_some(),
                }),
            )
            .await;
        }
        for process_id in cleanup_processes {
            let cleaned = self.cleanup_process(process_id).await;
            self.record_event(
                "lease_process_cleanup",
                None,
                None,
                None,
                json!({
                    "process_id": process_id,
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

    async fn cleanup_process(&self, process_id: i32) -> bool {
        let cleaner = self.process_cleaner.read().await.clone();
        if let Some(cleaner) = cleaner {
            cleaner.cleanup_agent_os_process(process_id).await
        } else {
            false
        }
    }

    async fn create_artifact(
        &self,
        task_id: String,
        owner_thread_id: ThreadId,
        artifact_type: ArtifactType,
        uri: String,
        summary: String,
        metadata: serde_json::Value,
    ) -> PraxisResult<String> {
        let artifact = ArtifactRecord {
            artifact_id: format!("artifact-{}", Uuid::new_v4()),
            task_id,
            owner_thread_id,
            artifact_type,
            uri,
            summary,
            metadata,
            created_at: Utc::now(),
        };
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
            Some(owner_thread_id),
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

    async fn command_id_for_process(&self, process_id: i32) -> Option<String> {
        self.state
            .read()
            .await
            .commands
            .values()
            .find(|command| command.process_id == Some(process_id) && command.ended_at.is_none())
            .map(|command| command.command_id.clone())
    }

    async fn renew_command_leases(&self, command: &CommandRecord) {
        let snapshots = {
            let mut state = self.state.write().await;
            command
                .lease_ids
                .iter()
                .filter_map(|lease_id| {
                    let lease = state.leases.get_mut(lease_id)?;
                    lease.expires_at =
                        Some(Utc::now() + Duration::seconds(DEFAULT_LEASE_TTL_SECONDS));
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
            && (ticket.coordinator_epoch != active.epoch
                || ticket.fencing_token != active.fencing_token)
        {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket coordinator epoch is stale".to_string(),
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
        let Ok(snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS ticket snapshot: {err}");
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

    async fn persist_runtime_command_snapshot(&self, command: &RuntimeCommand) {
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
        1 => "builder",
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
            command_allowlist: Vec::new(),
            command_denylist: dangerous_command_denylist(),
        },
        CapabilityProfile {
            profile_id: "builder".to_string(),
            can_read_files: true,
            can_write_files: true,
            can_run_shell: true,
            can_cpu_heavy: true,
            can_compile: true,
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
                deny: vec![ActionIntentKind::RunApp, ActionIntentKind::GitMutation],
            },
            command_allowlist: Vec::new(),
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
            command_allowlist: Vec::new(),
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
    if summary.len() > 2_000 {
        summary.truncate(2_000);
    }
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

fn capacity_for_requirement(requirement: &ResourceRequirement) -> usize {
    match requirement {
        ResourceRequirement::CpuHeavy => 1,
        ResourceRequirement::LlmBudget { .. } => 8,
        _ => 1,
    }
}

fn normalize_path_for_scope(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn normalize_scope_pattern(pattern: &str) -> String {
    pattern
        .trim_start_matches("repo:")
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn scope_matches(pattern: &str, value: &str) -> bool {
    let pattern = normalize_scope_pattern(pattern);
    if pattern == "*" || pattern == "**" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return value.contains(prefix);
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return value.contains(prefix);
    }
    value.contains(pattern.as_str())
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

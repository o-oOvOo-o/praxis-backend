use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
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
use crate::path_scope::normalize_path_for_scope;
#[cfg(test)]
use crate::path_scope::scope_matches;
use praxis_rollout::StateDbHandle;

mod artifact_blobs;
mod artifacts;
mod capability;
mod classification;
mod commands;
mod coordination;
mod dirty;
mod intent;
mod leases;
mod model;
mod paths;
mod persistence;
mod policy;
mod process;
mod queries;
mod runtime_commands;
mod runtime_lifecycle;
mod tasks;
mod tickets;
mod worker_requests;

pub(crate) use classification::{
    classify_command, coordination_scope_for_session_source, profile_for_rank,
    rank_for_session_source,
};
pub(crate) use model::*;
pub(crate) use process::{AgentOsProcessCleaner, process_runtime_kind, process_runtime_owner};

use artifacts::{append_spool_stream, metadata_with_blob, sanitize_artifact_extension};
#[cfg(test)]
use classification::task_resource_allows;
use classification::{
    artifact_type_for_intent, capacity_for_requirement, classify_mutating_tool,
    requires_dirty_audit, runtime_kind_for_intent, summarize_output, validate_task_action_contract,
};
use dirty::{
    audit_git_dirty_files, dirty_file_allowed_by_task, dirty_file_delta, dirty_file_fingerprints,
    format_dirty_file_report, push_unique_dirty_files,
};
use model::{ActiveCoordinatorLease, DirtyFileFingerprint, RuntimeCommandActivity};
use paths::action_fingerprint;
use policy::{
    AgentOsPolicy, COORDINATOR_RANK, HARD_ARTIFACT_READ_MAX_BYTES, LEASE_JANITOR_INTERVAL_SECONDS,
    MAX_COORDINATORS,
};
use process::{cleaner_registry_key, process_registry_key};

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

    #[test]
    fn classify_command_keeps_fd_merge_search_read_only() {
        let command = vec![
            "powershell.exe".to_string(),
            "-Command".to_string(),
            "rg -n \"Ridge\" crates/cunning_core/src/bin/main.rs 2>&1".to_string(),
        ];

        let intent = classify_command(&command, Path::new("D:/repo"));

        assert_eq!(intent.kind, ActionIntentKind::ReadOnly);
        assert!(intent.required_resources.is_empty());
    }

    #[test]
    fn classify_command_treats_file_redirection_as_write() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "printf 'export const x = 1' > src/index.ts".to_string(),
        ];

        let intent = classify_command(&command, Path::new("/repo"));

        assert_eq!(intent.kind, ActionIntentKind::FileWrite);
        assert!(
            intent
                .required_resources
                .iter()
                .any(|resource| matches!(resource, ResourceRequirement::RepoWrite { .. }))
        );
    }
}

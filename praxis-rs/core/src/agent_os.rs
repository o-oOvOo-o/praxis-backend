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
mod control_plane;
mod coordination;
mod dirty;
mod intent;
mod leases;
mod lifecycle;
mod managed_commands;
mod model;
mod paths;
mod persistence;
mod policy;
mod process;
mod read_model;
mod runtime_commands;
mod state;
mod tasks;
mod tickets;
mod worker_requests;

pub(crate) use classification::classify_command;
pub(crate) use classification::coordination_scope_for_session_source;
pub(crate) use classification::profile_for_rank;
pub(crate) use classification::rank_for_session_source;
pub(crate) use commands::AgentOsExecutionOpenRequest;
pub(crate) use control_plane::AgentTaskDispatchRequest;
pub(crate) use managed_commands::ManagedCommandSpan;
pub(crate) use model::*;
pub(crate) use process::AgentOsProcessCleaner;
pub(crate) use process::process_runtime_kind;
pub(crate) use process::process_runtime_owner;
pub(crate) use read_model::AgentOsEventBatch;
pub(crate) use read_model::AgentOsEventQuery;
pub(crate) use read_model::AgentOsSnapshot;
pub(crate) use read_model::AgentOsSnapshotOptions;

use artifacts::append_spool_stream;
use artifacts::metadata_with_blob;
use artifacts::sanitize_artifact_extension;
use classification::artifact_type_for_intent;
use classification::capacity_for_requirement;
use classification::classify_mutating_tool;
use classification::requires_dirty_audit;
use classification::runtime_kind_for_intent;
use classification::summarize_output;
#[cfg(test)]
use classification::task_resource_allows;
use classification::validate_task_action_contract;
use dirty::audit_git_dirty_files;
use dirty::dirty_file_allowed_by_task;
use dirty::dirty_file_delta;
use dirty::dirty_file_fingerprints;
use dirty::format_dirty_file_report;
use dirty::push_unique_dirty_files;
use managed_commands::DirtyAuditOutcome;
use managed_commands::ManagedCommandOutputSource;
use model::DirtyFileFingerprint;
use model::RuntimeCommandActivity;
use paths::action_fingerprint;
use policy::AgentOsPolicy;
use policy::COORDINATOR_RANK;
use policy::HARD_ARTIFACT_READ_MAX_BYTES;
use policy::LEASE_JANITOR_INTERVAL_SECONDS;
use policy::MAX_COORDINATORS;
use process::cleaner_registry_key;
use process::process_registry_key;
use state::AgentOsState;
use state::has_active_assign_runtime_command_locked;

pub(crate) struct AgentOs {
    state: RwLock<AgentOsState>,
    state_db: RwLock<Option<StateDbHandle>>,
    // Multiple sessions share one AgentOS instance. Cleaners are indexed by runtime
    // kind so lease expiry can route process cleanup to the backend that owns the
    // process instead of guessing through every session-level manager.
    process_cleaners: RwLock<HashMap<String, Vec<Arc<dyn AgentOsProcessCleaner>>>>,
    process_cleaners_by_owner: RwLock<HashMap<String, Arc<dyn AgentOsProcessCleaner>>>,
    lease_janitor_started: AtomicBool,
    change_seq: AtomicU64,
    change_tx: watch::Sender<u64>,
}

impl Default for AgentOs {
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

impl AgentOs {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.change_tx.subscribe()
    }

    pub(crate) fn change_sequence(&self) -> u64 {
        self.change_seq.load(Ordering::SeqCst)
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

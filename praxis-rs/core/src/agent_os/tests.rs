use super::AgentOs;
use super::classification::classify_command;
use super::classification::task_resource_allows;
use super::records::ActionIntentKind;
use super::records::ResourceRequirement;
use super::records::RuntimeCommandStatus;
use super::records::ThreadRegistration;
use super::records::ThreadRuntimeState;
use crate::path_scope::scope_matches;
use praxis_protocol::ThreadId;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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

#[tokio::test]
async fn conflicting_lease_waits_for_release_instead_of_rejecting_parallel_tool() {
    let agent_os = AgentOs::new();
    let requirement = [ResourceRequirement::BuildCache {
        scope: "repo:shared".to_string(),
    }];
    let first_thread = ThreadId::new();
    let second_thread = ThreadId::new();
    let first = agent_os
        .acquire_required_leases(first_thread, "first", 0, &requirement)
        .await
        .expect("first lease should be acquired");

    let waiting_agent_os = Arc::clone(&agent_os);
    let mut waiting = tokio::spawn(async move {
        waiting_agent_os
            .acquire_required_leases(second_thread, "second", 0, &requirement)
            .await
    });

    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut waiting)
            .await
            .is_err(),
        "a conflicting parallel tool should wait instead of being rejected"
    );

    agent_os.release_leases(&first).await;
    let second = tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .expect("waiter should wake after release")
        .expect("waiter task should not panic")
        .expect("waiter should acquire the released capacity");
    assert_eq!(second.len(), 1);
    agent_os.release_leases(&second).await;
}

#[tokio::test]
async fn turn_without_runtime_command_cannot_fail_newly_assigned_task() {
    let agent_os = AgentOs::new();
    let coordinator = ThreadId::new();
    let worker = ThreadId::new();
    for (thread_id, rank) in [(coordinator, 0), (worker, 2)] {
        agent_os
            .register_thread(ThreadRegistration {
                thread_id,
                coordination_scope: "test-scope".to_string(),
                rank,
                profile_id: super::classification::profile_for_rank(rank).to_string(),
                cwd: PathBuf::from("F:/repo"),
                repo_id: None,
                branch: None,
                worktree: None,
                priority: 0,
            })
            .await
            .expect("thread should register");
    }

    let dispatch = agent_os
        .dispatch_task(super::AgentTaskDispatchRequest {
            from_thread_id: coordinator,
            to_thread_id: worker,
            prompt: "new task".to_string(),
            objective: "new task".to_string(),
            scope: vec!["F:/repo".to_string()],
            constraints: Vec::new(),
            acceptance_criteria: Vec::new(),
            artifact_refs: Vec::new(),
            required_capabilities: Vec::new(),
            required_resources: Vec::new(),
            token_budget: None,
            priority: 0,
            exploratory: false,
            interrupt: true,
        })
        .await
        .expect("task should dispatch");

    let completed = agent_os
        .complete_runtime_command_for_turn(
            worker,
            None,
            /*succeeded*/ false,
            "stale turn aborted",
        )
        .await
        .expect("stale completion should be ignored");

    assert!(completed.is_none());
    let state = agent_os.state.read().await;
    assert_eq!(
        state
            .threads
            .get(&worker)
            .and_then(|thread| thread.current_task_id.as_deref()),
        Some(dispatch.task_id.as_str())
    );
    assert_eq!(
        state
            .runtime_commands
            .get(&dispatch.runtime_command.command_id)
            .map(|command| command.status),
        Some(RuntimeCommandStatus::Pending)
    );
}

#[tokio::test]
async fn stale_runtime_command_completion_preserves_new_task_thread_state() {
    let agent_os = AgentOs::new();
    let coordinator = ThreadId::new();
    let worker = ThreadId::new();
    for (thread_id, rank) in [(coordinator, 0), (worker, 2)] {
        agent_os
            .register_thread(ThreadRegistration {
                thread_id,
                coordination_scope: "test-scope".to_string(),
                rank,
                profile_id: super::classification::profile_for_rank(rank).to_string(),
                cwd: PathBuf::from("F:/repo"),
                repo_id: None,
                branch: None,
                worktree: None,
                priority: 0,
            })
            .await
            .expect("thread should register");
    }

    let old_dispatch = agent_os
        .dispatch_task(super::AgentTaskDispatchRequest {
            from_thread_id: coordinator,
            to_thread_id: worker,
            prompt: "old task".to_string(),
            objective: "old task".to_string(),
            scope: vec!["F:/repo".to_string()],
            constraints: Vec::new(),
            acceptance_criteria: Vec::new(),
            artifact_refs: Vec::new(),
            required_capabilities: Vec::new(),
            required_resources: Vec::new(),
            token_budget: None,
            priority: 0,
            exploratory: false,
            interrupt: false,
        })
        .await
        .expect("old task should dispatch");
    agent_os
        .claim_runtime_commands_for_turn(worker)
        .await
        .expect("old command should be claimed");

    let new_dispatch = agent_os
        .dispatch_task(super::AgentTaskDispatchRequest {
            from_thread_id: coordinator,
            to_thread_id: worker,
            prompt: "new task".to_string(),
            objective: "new task".to_string(),
            scope: vec!["F:/repo".to_string()],
            constraints: Vec::new(),
            acceptance_criteria: Vec::new(),
            artifact_refs: Vec::new(),
            required_capabilities: Vec::new(),
            required_resources: Vec::new(),
            token_budget: None,
            priority: 0,
            exploratory: false,
            interrupt: true,
        })
        .await
        .expect("new task should dispatch");

    agent_os
        .complete_runtime_command_for_turn(
            worker,
            Some(old_dispatch.runtime_command.command_id.as_str()),
            /*succeeded*/ false,
            "old turn aborted after replacement",
        )
        .await
        .expect("old command should complete by exact id");

    let state = agent_os.state.read().await;
    let worker_state = state.threads.get(&worker).expect("worker should remain");
    assert_eq!(
        worker_state.current_task_id.as_deref(),
        Some(new_dispatch.task_id.as_str())
    );
    assert_eq!(worker_state.state, ThreadRuntimeState::Assigned);
    assert_eq!(
        state
            .runtime_commands
            .get(&new_dispatch.runtime_command.command_id)
            .map(|command| command.status),
        Some(RuntimeCommandStatus::Pending)
    );
}

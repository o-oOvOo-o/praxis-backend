use super::classification::classify_command;
use super::classification::task_resource_allows;
use super::records::ActionIntentKind;
use super::records::ResourceRequirement;
use crate::path_scope::scope_matches;
use std::path::Path;

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

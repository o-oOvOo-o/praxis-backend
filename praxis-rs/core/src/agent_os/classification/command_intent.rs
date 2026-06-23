use std::path::Path;

use crate::agent_os::records::ActionIntent;
use crate::agent_os::records::ActionIntentKind;
use crate::agent_os::records::ResourceRequirement;
use crate::agent_os::paths::repo_scope_for_cwd;

use super::command_matchers::*;

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
    } else if is_file_write_command(&rendered) || has_file_redirection(&rendered) {
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

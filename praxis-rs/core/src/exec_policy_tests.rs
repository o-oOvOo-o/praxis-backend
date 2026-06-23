use super::*;
use crate::config::Config;
use crate::config::ConfigBuilder;
use crate::config_loader::ConfigLayerEntry;
use crate::config_loader::ConfigLayerStack;
use crate::config_loader::ConfigLayerStackOrdering;
use crate::config_loader::ConfigRequirements;
use crate::config_loader::ConfigRequirementsToml;
use crate::config_loader::LoaderOverrides;
use crate::config_loader::RequirementSource;
use crate::config_loader::Sourced;
use praxis_config::RequirementsExecPolicy;
use praxis_protocol::config_layers::ConfigLayerSource;
use praxis_protocol::permissions::FileSystemAccessMode;
use praxis_protocol::permissions::FileSystemPath;
use praxis_protocol::permissions::FileSystemSandboxEntry;
use praxis_protocol::permissions::FileSystemSpecialPath;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::GranularApprovalConfig;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tempfile::tempdir;
use toml::Value as TomlValue;

fn config_stack_for_dot_praxis_folder(dot_praxis_folder: &Path) -> ConfigLayerStack {
    let dot_praxis_folder =
        AbsolutePathBuf::from_absolute_path(dot_praxis_folder).expect("absolute dot_praxis_folder");
    let layer = ConfigLayerEntry::new(
        ConfigLayerSource::Project { dot_praxis_folder },
        TomlValue::Table(Default::default()),
    );
    ConfigLayerStack::new(
        vec![layer],
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )
    .expect("ConfigLayerStack")
}

fn host_absolute_path(segments: &[&str]) -> String {
    let mut path = if cfg!(windows) {
        PathBuf::from(r"C:\")
    } else {
        PathBuf::from("/")
    };
    for segment in segments {
        path.push(segment);
    }
    path.to_string_lossy().into_owned()
}

fn host_program_path(name: &str) -> String {
    let executable_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    host_absolute_path(&["usr", "bin", &executable_name])
}

fn starlark_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn read_only_file_system_sandbox_policy() -> FileSystemSandboxPolicy {
    FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        access: FileSystemAccessMode::Read,
    }])
}

fn unrestricted_file_system_sandbox_policy() -> FileSystemSandboxPolicy {
    FileSystemSandboxPolicy::unrestricted()
}

fn external_file_system_sandbox_policy() -> FileSystemSandboxPolicy {
    FileSystemSandboxPolicy::external_sandbox()
}

async fn test_config() -> (TempDir, Config) {
    let home = TempDir::new().expect("create temp dir");
    let config = ConfigBuilder::default()
        .praxis_home(home.path().to_path_buf())
        .loader_overrides(LoaderOverrides {
            #[cfg(target_os = "macos")]
            managed_preferences_base64: Some(String::new()),
            macos_managed_config_requirements_base64: Some(String::new()),
            ..LoaderOverrides::default()
        })
        .build()
        .await
        .expect("load default test config");
    (home, config)
}

fn derive_requested_execpolicy_amendment_for_test(
    prefix_rule: Option<&Vec<String>>,
    matched_rules: &[RuleMatch],
) -> Option<ExecPolicyAmendment> {
    let commands = prefix_rule
        .cloned()
        .map(|prefix_rule| vec![prefix_rule])
        .unwrap_or_else(|| vec![vec!["echo".to_string()]]);
    derive_requested_execpolicy_amendment_from_prefix_rule(
        prefix_rule,
        matched_rules,
        &Policy::empty(),
        &commands,
        &|_: &[String]| Decision::Allow,
        &MatchOptions::default(),
    )
}

fn vec_str(items: &[&str]) -> Vec<String> {
    items.iter().map(std::string::ToString::to_string).collect()
}

/// Note this test behaves differently on Windows because it exercises an
/// `if cfg!(windows)` code path in render_decision_for_unmatched_command().
struct ExecApprovalRequirementScenario {
    /// Source for the Starlark `.rules` file.
    policy_src: Option<String>,
    command: Vec<String>,
    approval_policy: AskForApproval,
    sandbox_policy: SandboxPolicy,
    file_system_sandbox_policy: FileSystemSandboxPolicy,
    sandbox_permissions: SandboxPermissions,
    prefix_rule: Option<Vec<String>>,
}

async fn assert_exec_approval_requirement_for_command(
    test: ExecApprovalRequirementScenario,
    expected_requirement: ExecApprovalRequirement,
) {
    let ExecApprovalRequirementScenario {
        policy_src,
        command,
        approval_policy,
        sandbox_policy,
        file_system_sandbox_policy,
        sandbox_permissions,
        prefix_rule,
    } = test;

    let policy = match policy_src {
        Some(src) => {
            let mut parser = PolicyParser::new();
            parser
                .parse("test.rules", src.as_str())
                .expect("parse policy");
            Arc::new(parser.build())
        }
        None => Arc::new(Policy::empty()),
    };

    let requirement = ExecPolicyManager::new(policy)
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy,
            sandbox_policy: &sandbox_policy,
            file_system_sandbox_policy: &file_system_sandbox_policy,
            sandbox_permissions,
            prefix_rule,
        })
        .await;

    assert_eq!(requirement, expected_requirement);
}

#[path = "exec_policy_tests/amendments.rs"]
mod amendments;
#[path = "exec_policy_tests/approval_requirements.rs"]
mod approval_requirements;
#[path = "exec_policy_tests/command_parsing.rs"]
mod command_parsing;
#[path = "exec_policy_tests/dangerous_commands.rs"]
mod dangerous_commands;
#[path = "exec_policy_tests/derived_amendments.rs"]
mod derived_amendments;
#[path = "exec_policy_tests/policy_loading.rs"]
mod policy_loading;

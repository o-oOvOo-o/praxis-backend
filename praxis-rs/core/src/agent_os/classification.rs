use super::ActionIntent;
use super::ActionIntentKind;
use super::ArtifactType;
use super::CapabilityProfile;
use super::ResourceRequirement;
use super::ScopedIntents;
use super::ScopedPaths;
use super::TaskRecord;
use super::paths::repo_scope_for_cwd;
use super::policy::COORDINATOR_RANK;
use super::process::process_runtime_kind;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::path_scope::wildcard_match;
use crate::util::truncate_to_char_boundary;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use std::collections::HashSet;
use std::path::Path;

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

pub(super) fn classify_mutating_tool(tool_name: &str) -> ActionIntent {
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

pub(super) fn builtin_profiles() -> Vec<CapabilityProfile> {
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

pub(super) fn denylist_surface(command: &[String]) -> String {
    if command
        .first()
        .is_some_and(|program| program.eq_ignore_ascii_case("apply_patch"))
    {
        return "apply_patch".to_string();
    }
    command.join(" ")
}

pub(super) fn runtime_kind_for_intent(intent: ActionIntentKind) -> &'static str {
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

pub(super) fn artifact_type_for_intent(intent: ActionIntentKind) -> ArtifactType {
    match intent {
        ActionIntentKind::Compile | ActionIntentKind::Test | ActionIntentKind::Harness => {
            ArtifactType::CompileLog
        }
        _ => ArtifactType::CommandLog,
    }
}

pub(super) fn summarize_output(raw_output: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw_output);
    let mut summary = text.lines().take(20).collect::<Vec<_>>().join("\n");
    truncate_to_char_boundary(&mut summary, 2_000);
    summary
}

pub(super) fn requires_write(intent: ActionIntentKind) -> bool {
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

pub(super) fn requires_dirty_audit(intent: ActionIntentKind) -> bool {
    requires_write(intent) || matches!(intent, ActionIntentKind::GitMutation)
}

pub(super) fn requires_compile(intent: ActionIntentKind) -> bool {
    matches!(intent, ActionIntentKind::Compile | ActionIntentKind::Test)
}

pub(super) fn requires_cpu_heavy(intent: ActionIntentKind) -> bool {
    matches!(
        intent,
        ActionIntentKind::Compile
            | ActionIntentKind::Test
            | ActionIntentKind::LongProcess
            | ActionIntentKind::UnknownRisky
    )
}

pub(super) fn validate_task_action_contract(
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

pub(super) fn task_resource_allows(
    declared: &ResourceRequirement,
    required: &ResourceRequirement,
) -> bool {
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

pub(super) fn capacity_for_requirement(requirement: &ResourceRequirement) -> usize {
    match requirement {
        ResourceRequirement::CpuHeavy => 1,
        ResourceRequirement::LlmBudget { .. } => 8,
        _ => 1,
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
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn has_file_redirection(command: &str) -> bool {
    let bytes = command.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'>' {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        if cursor < bytes.len() && bytes[cursor] == b'>' {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'&' {
            index = cursor + 1;
            continue;
        }
        return true;
    }
    false
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

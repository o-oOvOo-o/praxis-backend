use super::*;

fn scenarios() -> Vec<ScenarioSpec> {
    use AskForApproval::*;

    let workspace_write = |network_access| SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: Default::default(),
        network_access,
        exclude_tmpdir_env_var: false,
        exclude_slash_tmp: false,
    };

    vec![
        ScenarioSpec {
            name: "danger_full_access_on_request_allows_outside_write",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_on_request.txt"),
                content: "danger-on-request",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("dfa_on_request.txt"),
                content: "danger-on-request",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_on_request_allows_outside_write_gpt_5_1_no_exit",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_on_request_5_1.txt"),
                content: "danger-on-request",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("dfa_on_request_5_1.txt"),
                content: "danger-on-request",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_on_request_allows_network",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::FetchUrlNoProxy {
                endpoint: "/dfa/network",
                response_body: "danger-network-ok",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::NetworkSuccess {
                body_contains: "danger-network-ok",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_on_request_allows_network_gpt_5_1_no_exit",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::FetchUrlNoProxy {
                endpoint: "/dfa/network",
                response_body: "danger-network-ok",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::Auto,
            expectation: Expectation::NetworkSuccessNoExitCode {
                body_contains: "danger-network-ok",
            },
        },
        ScenarioSpec {
            name: "trusted_command_unless_trusted_runs_without_prompt",
            approval_policy: UnlessTrusted,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::RunCommand {
                command: "echo trusted-unless",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::CommandSuccess {
                stdout_contains: "trusted-unless",
            },
        },
        ScenarioSpec {
            name: "trusted_command_unless_trusted_runs_without_prompt_gpt_5_1_no_exit",
            approval_policy: UnlessTrusted,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::RunCommand {
                command: "echo trusted-unless",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::Auto,
            expectation: Expectation::CommandSuccessNoExitCode {
                stdout_contains: "trusted-unless",
            },
        },
        ScenarioSpec {
            name: "cat_redirect_unless_trusted_requires_approval",
            approval_policy: UnlessTrusted,
            sandbox_policy: workspace_write(false),
            action: ActionKind::RunCommand {
                command: r#"cat < "hello" > /var/test.txt"#,
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Denied,
                expected_reason: None,
            },
            expectation: Expectation::CommandFailure {
                output_contains: "rejected by user",
            },
        },
        ScenarioSpec {
            name: "cat_redirect_on_request_requires_approval",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::RunCommand {
                command: r#"cat < "hello" > /var/test.txt"#,
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Denied,
                expected_reason: None,
            },
            expectation: Expectation::CommandFailure {
                output_contains: "rejected by user",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_on_failure_allows_outside_write",
            approval_policy: OnFailure,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_on_failure.txt"),
                content: "danger-on-failure",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("dfa_on_failure.txt"),
                content: "danger-on-failure",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_on_failure_allows_outside_write_gpt_5_1_no_exit",
            approval_policy: OnFailure,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_on_failure_5_1.txt"),
                content: "danger-on-failure",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileCreatedNoExitCode {
                target: TargetPath::OutsideWorkspace("dfa_on_failure_5_1.txt"),
                content: "danger-on-failure",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_unless_trusted_requests_approval",
            approval_policy: UnlessTrusted,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_unless_trusted.txt"),
                content: "danger-unless-trusted",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("dfa_unless_trusted.txt"),
                content: "danger-unless-trusted",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_unless_trusted_requests_approval_gpt_5_1_no_exit",
            approval_policy: UnlessTrusted,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_unless_trusted_5_1.txt"),
                content: "danger-unless-trusted",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreatedNoExitCode {
                target: TargetPath::OutsideWorkspace("dfa_unless_trusted_5_1.txt"),
                content: "danger-unless-trusted",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_never_allows_outside_write",
            approval_policy: Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_never.txt"),
                content: "danger-never",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("dfa_never.txt"),
                content: "danger-never",
            },
        },
        ScenarioSpec {
            name: "danger_full_access_never_allows_outside_write_gpt_5_1_no_exit",
            approval_policy: Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("dfa_never_5_1.txt"),
                content: "danger-never",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileCreatedNoExitCode {
                target: TargetPath::OutsideWorkspace("dfa_never_5_1.txt"),
                content: "danger-never",
            },
        },
        ScenarioSpec {
            name: "read_only_on_request_requires_approval",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_on_request.txt"),
                content: "read-only-approval",
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreated {
                target: TargetPath::Workspace("ro_on_request.txt"),
                content: "read-only-approval",
            },
        },
        ScenarioSpec {
            name: "read_only_on_request_requires_approval_gpt_5_1_no_exit",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_on_request_5_1.txt"),
                content: "read-only-approval",
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreatedNoExitCode {
                target: TargetPath::Workspace("ro_on_request_5_1.txt"),
                content: "read-only-approval",
            },
        },
        ScenarioSpec {
            name: "trusted_command_on_request_read_only_runs_without_prompt",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::RunCommand {
                command: "echo trusted-read-only",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::CommandSuccess {
                stdout_contains: "trusted-read-only",
            },
        },
        ScenarioSpec {
            name: "trusted_command_on_request_read_only_runs_without_prompt_gpt_5_1_no_exit",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::RunCommand {
                command: "echo trusted-read-only",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::Auto,
            expectation: Expectation::CommandSuccessNoExitCode {
                stdout_contains: "trusted-read-only",
            },
        },
        ScenarioSpec {
            name: "read_only_on_request_blocks_network",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::FetchUrl {
                endpoint: "/ro/network-blocked",
                response_body: "should-not-see",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: None,
            outcome: Outcome::Auto,
            expectation: Expectation::NetworkFailure { expect_tag: "ERR:" },
        },
        ScenarioSpec {
            name: "read_only_on_request_denied_blocks_execution",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_on_request_denied.txt"),
                content: "should-not-write",
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![],
            model_override: None,
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Denied,
                expected_reason: None,
            },
            expectation: Expectation::FileNotCreated {
                target: TargetPath::Workspace("ro_on_request_denied.txt"),
                message_contains: &["exec command rejected by user"],
            },
        },
        #[cfg(not(target_os = "linux"))] // TODO (pakrym): figure out why linux behaves differently
        ScenarioSpec {
            name: "read_only_on_failure_escalates_after_sandbox_error",
            approval_policy: OnFailure,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_on_failure.txt"),
                content: "read-only-on-failure",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: Some("command failed; retry without sandbox?"),
            },
            expectation: Expectation::FileCreated {
                target: TargetPath::Workspace("ro_on_failure.txt"),
                content: "read-only-on-failure",
            },
        },
        #[cfg(not(target_os = "linux"))]
        ScenarioSpec {
            name: "read_only_on_failure_escalates_after_sandbox_error_gpt_5_1_no_exit",
            approval_policy: OnFailure,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_on_failure_5_1.txt"),
                content: "read-only-on-failure",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: Some("command failed; retry without sandbox?"),
            },
            expectation: Expectation::FileCreatedNoExitCode {
                target: TargetPath::Workspace("ro_on_failure_5_1.txt"),
                content: "read-only-on-failure",
            },
        },
        ScenarioSpec {
            name: "read_only_on_request_network_escalates_when_approved",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::FetchUrl {
                endpoint: "/ro/network-approved",
                response_body: "read-only-network-ok",
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::NetworkSuccess {
                body_contains: "read-only-network-ok",
            },
        },
        ScenarioSpec {
            name: "read_only_on_request_network_escalates_when_approved_gpt_5_1_no_exit",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::FetchUrl {
                endpoint: "/ro/network-approved",
                response_body: "read-only-network-ok",
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::NetworkSuccessNoExitCode {
                body_contains: "read-only-network-ok",
            },
        },
        ScenarioSpec {
            name: "apply_patch_shell_command_requires_patch_approval",
            approval_policy: UnlessTrusted,
            sandbox_policy: workspace_write(false),
            action: ActionKind::ApplyPatchShell {
                target: TargetPath::Workspace("apply_patch_shell.txt"),
                content: "shell-apply-patch",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: None,
            outcome: Outcome::PatchApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::PatchApplied {
                target: TargetPath::Workspace("apply_patch_shell.txt"),
                content: "shell-apply-patch",
            },
        },
        ScenarioSpec {
            name: "apply_patch_function_auto_inside_workspace",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::ApplyPatchFunction {
                target: TargetPath::Workspace("apply_patch_function.txt"),
                content: "function-apply-patch",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1-codex"),
            outcome: Outcome::Auto,
            expectation: Expectation::PatchApplied {
                target: TargetPath::Workspace("apply_patch_function.txt"),
                content: "function-apply-patch",
            },
        },
        ScenarioSpec {
            name: "apply_patch_function_danger_allows_outside_workspace",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::ApplyPatchFunction {
                target: TargetPath::OutsideWorkspace("apply_patch_function_danger.txt"),
                content: "function-patch-danger",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![Feature::ApplyPatchFreeform],
            model_override: Some("gpt-5.1-codex"),
            outcome: Outcome::Auto,
            expectation: Expectation::PatchApplied {
                target: TargetPath::OutsideWorkspace("apply_patch_function_danger.txt"),
                content: "function-patch-danger",
            },
        },
        ScenarioSpec {
            name: "apply_patch_function_outside_requires_patch_approval",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::ApplyPatchFunction {
                target: TargetPath::OutsideWorkspace("apply_patch_function_outside.txt"),
                content: "function-patch-outside",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1-codex"),
            outcome: Outcome::PatchApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::PatchApplied {
                target: TargetPath::OutsideWorkspace("apply_patch_function_outside.txt"),
                content: "function-patch-outside",
            },
        },
        ScenarioSpec {
            name: "apply_patch_function_outside_denied_blocks_patch",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::ApplyPatchFunction {
                target: TargetPath::OutsideWorkspace("apply_patch_function_outside_denied.txt"),
                content: "function-patch-outside-denied",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1-codex"),
            outcome: Outcome::PatchApproval {
                decision: ReviewDecision::Denied,
                expected_reason: None,
            },
            expectation: Expectation::FileNotCreated {
                target: TargetPath::OutsideWorkspace("apply_patch_function_outside_denied.txt"),
                message_contains: &["patch rejected by user"],
            },
        },
        ScenarioSpec {
            name: "apply_patch_shell_command_outside_requires_patch_approval",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::ApplyPatchShell {
                target: TargetPath::OutsideWorkspace("apply_patch_shell_outside.txt"),
                content: "shell-patch-outside",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: None,
            outcome: Outcome::PatchApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::PatchApplied {
                target: TargetPath::OutsideWorkspace("apply_patch_shell_outside.txt"),
                content: "shell-patch-outside",
            },
        },
        ScenarioSpec {
            name: "apply_patch_function_unless_trusted_requires_patch_approval",
            approval_policy: UnlessTrusted,
            sandbox_policy: workspace_write(false),
            action: ActionKind::ApplyPatchFunction {
                target: TargetPath::Workspace("apply_patch_function_unless_trusted.txt"),
                content: "function-patch-unless-trusted",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1-codex"),
            outcome: Outcome::PatchApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::PatchApplied {
                target: TargetPath::Workspace("apply_patch_function_unless_trusted.txt"),
                content: "function-patch-unless-trusted",
            },
        },
        ScenarioSpec {
            name: "apply_patch_function_never_rejects_outside_workspace",
            approval_policy: Never,
            sandbox_policy: workspace_write(false),
            action: ActionKind::ApplyPatchFunction {
                target: TargetPath::OutsideWorkspace("apply_patch_function_never.txt"),
                content: "function-patch-never",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1-codex"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileNotCreated {
                target: TargetPath::OutsideWorkspace("apply_patch_function_never.txt"),
                message_contains: &[
                    "patch rejected: writing outside of the project; rejected by user approval settings",
                ],
            },
        },
        ScenarioSpec {
            name: "read_only_unless_trusted_requires_approval",
            approval_policy: UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_unless_trusted.txt"),
                content: "read-only-unless-trusted",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreated {
                target: TargetPath::Workspace("ro_unless_trusted.txt"),
                content: "read-only-unless-trusted",
            },
        },
        ScenarioSpec {
            name: "read_only_unless_trusted_requires_approval_gpt_5_1_no_exit",
            approval_policy: UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_unless_trusted_5_1.txt"),
                content: "read-only-unless-trusted",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5.1"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreatedNoExitCode {
                target: TargetPath::Workspace("ro_unless_trusted_5_1.txt"),
                content: "read-only-unless-trusted",
            },
        },
        ScenarioSpec {
            name: "read_only_never_reports_sandbox_failure",
            approval_policy: Never,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ro_never.txt"),
                content: "read-only-never",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: None,
            outcome: Outcome::Auto,
            expectation: Expectation::FileNotCreated {
                target: TargetPath::Workspace("ro_never.txt"),
                message_contains: if cfg!(target_os = "linux") {
                    &["Permission denied|Read-only file system"]
                } else {
                    &[
                        "Permission denied|Operation not permitted|operation not permitted|\
                         Read-only file system",
                    ]
                },
            },
        },
        ScenarioSpec {
            name: "trusted_command_never_runs_without_prompt",
            approval_policy: Never,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::RunCommand {
                command: "echo trusted-never",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::CommandSuccess {
                stdout_contains: "trusted-never",
            },
        },
        ScenarioSpec {
            name: "workspace_write_on_request_allows_workspace_write",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::WriteFile {
                target: TargetPath::Workspace("ww_on_request.txt"),
                content: "workspace-on-request",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::FileCreated {
                target: TargetPath::Workspace("ww_on_request.txt"),
                content: "workspace-on-request",
            },
        },
        ScenarioSpec {
            name: "workspace_write_network_disabled_blocks_network",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::FetchUrl {
                endpoint: "/ww/network-blocked",
                response_body: "workspace-network-blocked",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: None,
            outcome: Outcome::Auto,
            expectation: Expectation::NetworkFailure { expect_tag: "ERR:" },
        },
        ScenarioSpec {
            name: "workspace_write_on_request_requires_approval_outside_workspace",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("ww_on_request_outside.txt"),
                content: "workspace-on-request-outside",
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("ww_on_request_outside.txt"),
                content: "workspace-on-request-outside",
            },
        },
        ScenarioSpec {
            name: "workspace_write_network_enabled_allows_network",
            approval_policy: OnRequest,
            sandbox_policy: workspace_write(true),
            action: ActionKind::FetchUrl {
                endpoint: "/ww/network-ok",
                response_body: "workspace-network-ok",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::NetworkSuccess {
                body_contains: "workspace-network-ok",
            },
        },
        #[cfg(not(target_os = "linux"))] // TODO (pakrym): figure out why linux behaves differently
        ScenarioSpec {
            name: "workspace_write_on_failure_escalates_outside_workspace",
            approval_policy: OnFailure,
            sandbox_policy: workspace_write(false),
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("ww_on_failure.txt"),
                content: "workspace-on-failure",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: Some("command failed; retry without sandbox?"),
            },
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("ww_on_failure.txt"),
                content: "workspace-on-failure",
            },
        },
        ScenarioSpec {
            name: "workspace_write_unless_trusted_requires_approval_outside_workspace",
            approval_policy: UnlessTrusted,
            sandbox_policy: workspace_write(false),
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("ww_unless_trusted.txt"),
                content: "workspace-unless-trusted",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: None,
            },
            expectation: Expectation::FileCreated {
                target: TargetPath::OutsideWorkspace("ww_unless_trusted.txt"),
                content: "workspace-unless-trusted",
            },
        },
        ScenarioSpec {
            name: "workspace_write_never_blocks_outside_workspace",
            approval_policy: Never,
            sandbox_policy: workspace_write(false),
            action: ActionKind::WriteFile {
                target: TargetPath::OutsideWorkspace("ww_never.txt"),
                content: "workspace-never",
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![],
            model_override: None,
            outcome: Outcome::Auto,
            expectation: Expectation::FileNotCreated {
                target: TargetPath::OutsideWorkspace("ww_never.txt"),
                message_contains: if cfg!(target_os = "linux") {
                    &["Permission denied|Read-only file system"]
                } else {
                    &[
                        "Permission denied|Operation not permitted|operation not permitted|\
                         Read-only file system",
                    ]
                },
            },
        },
        ScenarioSpec {
            name: "unified exec on request no approval for safe command",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::RunUnifiedExecCommand {
                command: "echo \"hello unified exec\"",
                justification: None,
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![Feature::UnifiedExec],
            model_override: Some("gpt-5"),
            outcome: Outcome::Auto,
            expectation: Expectation::CommandSuccess {
                stdout_contains: "hello unified exec",
            },
        },
        #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
        // Linux sandbox arg0 test workaround doesn't work on ARM
        ScenarioSpec {
            name: "unified exec on request escalated requires approval",
            approval_policy: OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            action: ActionKind::RunUnifiedExecCommand {
                command: "python3 -c 'print('\"'\"'escalated unified exec'\"'\"')'",
                justification: Some(DEFAULT_UNIFIED_EXEC_JUSTIFICATION),
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![Feature::UnifiedExec],
            model_override: Some("gpt-5"),
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Approved,
                expected_reason: Some(DEFAULT_UNIFIED_EXEC_JUSTIFICATION),
            },
            expectation: Expectation::CommandSuccess {
                stdout_contains: "escalated unified exec",
            },
        },
        ScenarioSpec {
            name: "unified exec on request requires approval unless trusted",
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            action: ActionKind::RunUnifiedExecCommand {
                command: "git reset --hard",
                justification: None,
            },
            sandbox_permissions: SandboxPermissions::UseDefault,
            features: vec![Feature::UnifiedExec],
            model_override: None,
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Denied,
                expected_reason: None,
            },
            expectation: Expectation::CommandFailure {
                output_contains: "rejected by user",
            },
        },
        ScenarioSpec {
            name: "safe command with heredoc and redirect still requires approval",
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::RunUnifiedExecCommand {
                command: "cat <<'EOF' > /tmp/out.txt \nhello\nEOF",
                justification: None,
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![Feature::UnifiedExec],
            model_override: None,
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Denied,
                expected_reason: None,
            },
            expectation: Expectation::CommandFailure {
                output_contains: "rejected by user",
            },
        },
        ScenarioSpec {
            name: "compound command with one safe command still requires approval",
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: workspace_write(false),
            action: ActionKind::RunUnifiedExecCommand {
                command: "cat ./one.txt && touch ./two.txt",
                justification: None,
            },
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            features: vec![Feature::UnifiedExec],
            model_override: None,
            outcome: Outcome::ExecApproval {
                decision: ReviewDecision::Denied,
                expected_reason: None,
            },
            expectation: Expectation::CommandFailure {
                output_contains: "rejected by user",
            },
        },
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approval_matrix_covers_all_modes() -> Result<()> {
    skip_if_no_network!(Ok(()));

    for scenario in scenarios() {
        run_scenario(&scenario).await?;
    }

    Ok(())
}

async fn run_scenario(scenario: &ScenarioSpec) -> Result<()> {
    eprintln!("running approval scenario: {}", scenario.name);
    let server = start_mock_server().await;
    let approval_policy = scenario.approval_policy;
    let sandbox_policy = scenario.sandbox_policy.clone();
    let features = scenario.features.clone();
    let model_override = scenario.model_override;
    let model = model_override.unwrap_or("gpt-5.1");

    let mut builder = test_praxis().with_model(model).with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy.clone());
        for feature in features {
            config
                .features
                .enable(feature)
                .expect("test config should allow feature update");
        }
    });
    let test = builder.build(&server).await?;

    let call_id = scenario.name;
    let (event, expected_command) = scenario
        .action
        .prepare(&test, &server, call_id, scenario.sandbox_permissions)
        .await?;
    if let Some(command) = expected_command.as_deref() {
        eprintln!("approval scenario {} command: {command}", scenario.name);
    }

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            event,
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let results_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        scenario.name,
        scenario.approval_policy,
        scenario.sandbox_policy.clone(),
    )
    .await?;

    match &scenario.outcome {
        Outcome::Auto => {
            wait_for_completion_without_approval(&test).await;
        }
        Outcome::ExecApproval {
            decision,
            expected_reason,
        } => {
            let command = expected_command
                .as_deref()
                .expect("exec approval requires shell command");
            let approval = expect_exec_approval(&test, command).await;
            if let Some(expected_reason) = expected_reason {
                assert_eq!(
                    approval.reason.as_deref(),
                    Some(*expected_reason),
                    "unexpected approval reason for {}",
                    scenario.name
                );
            }
            test.thread
                .submit(Op::ExecApproval {
                    id: approval.effective_approval_id(),
                    turn_id: None,
                    decision: decision.clone(),
                })
                .await?;
            wait_for_completion(&test).await;
        }
        Outcome::PatchApproval {
            decision,
            expected_reason,
        } => {
            let approval = expect_patch_approval(&test, call_id).await;
            if let Some(expected_reason) = expected_reason {
                assert_eq!(
                    approval.reason.as_deref(),
                    Some(*expected_reason),
                    "unexpected patch approval reason for {}",
                    scenario.name
                );
            }
            test.thread
                .submit(Op::PatchApproval {
                    id: approval.call_id,
                    decision: decision.clone(),
                })
                .await?;
            wait_for_completion(&test).await;
        }
    }

    let output_item = results_mock.single_request().function_call_output(call_id);
    let result = parse_result(&output_item);
    eprintln!(
        "approval scenario {} result: exit_code={:?} stdout={:?}",
        scenario.name, result.exit_code, result.stdout
    );
    scenario.expectation.verify(&test, &result)?;

    Ok(())
}

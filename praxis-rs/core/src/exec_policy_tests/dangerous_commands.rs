use super::*;

#[tokio::test]
async fn dangerous_rm_rf_requires_approval_in_danger_full_access() {
    let command = vec_str(&["rm", "-rf", "/tmp/nonexistent"]);

    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: command.clone(),
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command)),
        },
    )
    .await;
}

#[tokio::test]
async fn verify_approval_requirement_for_unsafe_powershell_command() {
    // `brew install powershell` to run this test on a Mac!
    // Note `pwsh` is required to parse a PowerShell command to see if it
    // is safe.
    if which::which("pwsh").is_err() {
        return;
    }

    let policy = ExecPolicyManager::new(Arc::new(Policy::empty()));
    let permissions = SandboxPermissions::UseDefault;

    // This command should not be run without user approval unless there is
    // a proper sandbox in place to ensure safety.
    let sneaky_command = vec_str(&["pwsh", "-Command", "echo hi @(calc)"]);
    let expected_amendment = Some(ExecPolicyAmendment::new(vec_str(&[
        "pwsh",
        "-Command",
        "echo hi @(calc)",
    ])));
    let (pwsh_approval_reason, expected_req) = if cfg!(windows) {
        (
            r#"On Windows, SandboxPolicy::ReadOnly should be assumed to mean
                that no sandbox is present, so anything that is not "provably
                safe" should require approval."#,
            ExecApprovalRequirement::NeedsApproval {
                reason: None,
                proposed_execpolicy_amendment: expected_amendment.clone(),
            },
        )
    } else {
        (
            "On non-Windows, rely on the read-only sandbox to prevent harm.",
            ExecApprovalRequirement::Skip {
                bypass_sandbox: false,
                proposed_execpolicy_amendment: expected_amendment.clone(),
            },
        )
    };
    assert_eq!(
        expected_req,
        policy
            .create_exec_approval_requirement_for_command(ExecApprovalRequest {
                command: &sneaky_command,
                approval_policy: AskForApproval::OnRequest,
                sandbox_policy: &SandboxPolicy::new_read_only_policy(),
                file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
                sandbox_permissions: permissions,
                prefix_rule: None,
            })
            .await,
        "{pwsh_approval_reason}"
    );

    // This is flagged as a dangerous command on all platforms.
    let dangerous_command = vec_str(&["rm", "-rf", "/important/data"]);
    assert_eq!(
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec_str(&[
                "rm",
                "-rf",
                "/important/data",
            ]))),
        },
        policy
            .create_exec_approval_requirement_for_command(ExecApprovalRequest {
                command: &dangerous_command,
                approval_policy: AskForApproval::OnRequest,
                sandbox_policy: &SandboxPolicy::new_read_only_policy(),
                file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
                sandbox_permissions: permissions,
                prefix_rule: None,
            })
            .await,
        r#"On all platforms, a forbidden command should require approval
            (unless AskForApproval::Never is specified)."#
    );

    // A dangerous command should be forbidden if the user has specified
    // AskForApproval::Never.
    assert_eq!(
        ExecApprovalRequirement::Forbidden {
            reason: "`rm -rf /important/data` rejected: blocked by policy".to_string(),
        },
        policy
            .create_exec_approval_requirement_for_command(ExecApprovalRequest {
                command: &dangerous_command,
                approval_policy: AskForApproval::Never,
                sandbox_policy: &SandboxPolicy::new_read_only_policy(),
                file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
                sandbox_permissions: permissions,
                prefix_rule: None,
            })
            .await,
        r#"On all platforms, a forbidden command should require approval
            (unless AskForApproval::Never is specified)."#
    );
}

#[tokio::test]
async fn dangerous_command_allowed_when_sandbox_is_explicitly_disabled() {
    let command = vec_str(&["rm", "-rf", "/tmp/nonexistent"]);
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ExternalSandbox {
                network_access: Default::default(),
            },
            file_system_sandbox_policy: external_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment {
                command: vec_str(&["rm", "-rf", "/tmp/nonexistent"]),
            }),
        },
    )
    .await;
}

#[tokio::test]
async fn dangerous_command_forbidden_in_external_sandbox_when_policy_matches() {
    let command = vec_str(&["rm", "-rf", "/tmp/nonexistent"]);
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some("prefix_rule(pattern=['rm'], decision='prompt')".to_string()),
            command,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ExternalSandbox {
                network_access: Default::default(),
            },
            file_system_sandbox_policy: external_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Forbidden {
            reason: "approval required by policy, but AskForApproval is set to Never".to_string(),
        },
    )
    .await;
}

use super::*;

#[tokio::test]
async fn evaluates_bash_lc_inner_commands() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(r#"prefix_rule(pattern=["rm"], decision="forbidden")"#.to_string()),
            command: vec![
                "bash".to_string(),
                "-lc".to_string(),
                "rm -rf /some/important/folder".to_string(),
            ],
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Forbidden {
            reason: "`bash -lc 'rm -rf /some/important/folder'` rejected: policy forbids commands starting with `rm`".to_string(),
        },
    )
    .await;
}

#[test]
fn commands_for_exec_policy_falls_back_for_empty_shell_script() {
    let command = vec!["bash".to_string(), "-lc".to_string(), "".to_string()];

    assert_eq!(commands_for_exec_policy(&command), (vec![command], false));
}

#[test]
fn commands_for_exec_policy_falls_back_for_whitespace_shell_script() {
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "  \n\t  ".to_string(),
    ];

    assert_eq!(commands_for_exec_policy(&command), (vec![command], false));
}

#[tokio::test]
async fn evaluates_heredoc_script_against_prefix_rules() {
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "python3 <<'PY'\nprint('hello')\nPY".to_string(),
    ];

    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(r#"prefix_rule(pattern=["python3"], decision="allow")"#.to_string()),
            command,
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Skip {
            bypass_sandbox: true,
            proposed_execpolicy_amendment: None,
        },
    )
    .await;
}

#[tokio::test]
async fn omits_auto_amendment_for_heredoc_fallback_prompts() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: vec![
                "bash".to_string(),
                "-lc".to_string(),
                "python3 <<'PY'\nprint('hello')\nPY".to_string(),
            ],
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: None,
        },
    )
    .await;
}

#[tokio::test]
async fn drops_requested_amendment_for_heredoc_fallback_prompts_when_it_wont_match() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: vec![
                "bash".to_string(),
                "-lc".to_string(),
                "python3 <<'PY'\nprint('hello')\nPY".to_string(),
            ],
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: Some(vec![
                "python3".to_string(),
                "-m".to_string(),
                "pip".to_string(),
            ]),
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: None,
        },
    )
    .await;
}

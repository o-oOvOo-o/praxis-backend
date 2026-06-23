use super::*;

#[tokio::test]
async fn append_execpolicy_amendment_updates_policy_and_file() {
    let praxis_home = tempdir().expect("create temp dir");
    let prefix = vec!["echo".to_string(), "hello".to_string()];
    let manager = ExecPolicyManager::default();

    manager
        .append_amendment_and_update(praxis_home.path(), &ExecPolicyAmendment::from(prefix))
        .await
        .expect("update policy");
    let updated_policy = manager.current();

    let evaluation = updated_policy.check(
        &["echo".to_string(), "hello".to_string(), "world".to_string()],
        &|_| Decision::Allow,
    );
    assert!(matches!(
        evaluation,
        Evaluation {
            decision: Decision::Allow,
            ..
        }
    ));

    let contents = fs::read_to_string(default_policy_path(praxis_home.path()))
        .expect("policy file should have been created");
    assert_eq!(
        contents,
        r#"prefix_rule(pattern=["echo", "hello"], decision="allow")
"#
    );
}

#[tokio::test]
async fn append_execpolicy_amendment_rejects_empty_prefix() {
    let praxis_home = tempdir().expect("create temp dir");
    let manager = ExecPolicyManager::default();

    let result = manager
        .append_amendment_and_update(praxis_home.path(), &ExecPolicyAmendment::from(vec![]))
        .await;

    assert!(matches!(
        result,
        Err(ExecPolicyUpdateError::AppendRule {
            source: AmendError::EmptyPrefix,
            ..
        })
    ));
}

#[tokio::test]
async fn proposed_execpolicy_amendment_is_present_for_single_command_without_policy_match() {
    let command = vec!["cargo".to_string(), "build".to_string()];

    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: command.clone(),
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
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
async fn proposed_execpolicy_amendment_is_omitted_when_policy_prompts() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(r#"prefix_rule(pattern=["rm"], decision="prompt")"#.to_string()),
            command: vec!["rm".to_string()],
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: Some("`rm` requires approval by policy".to_string()),
            proposed_execpolicy_amendment: None,
        },
    )
    .await;
}

#[tokio::test]
async fn proposed_execpolicy_amendment_is_present_for_multi_command_scripts() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: vec![
                "bash".to_string(),
                "-lc".to_string(),
                "cargo build && echo ok".to_string(),
            ],
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                "cargo".to_string(),
                "build".to_string(),
            ])),
        },
    )
    .await;
}

#[tokio::test]
async fn proposed_execpolicy_amendment_uses_first_no_match_in_multi_command_scripts() {
    let policy_src = r#"prefix_rule(pattern=["cat"], decision="allow")"#;
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "cat && apple".to_string(),
    ];

    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(policy_src.to_string()),
            command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                "apple".to_string(),
            ])),
        },
    )
    .await;
}

#[tokio::test]
async fn proposed_execpolicy_amendment_is_present_when_heuristics_allow() {
    let command = vec!["echo".to_string(), "safe".to_string()];

    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: command.clone(),
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command)),
        },
    )
    .await;
}

#[tokio::test]
async fn proposed_execpolicy_amendment_is_suppressed_when_policy_matches_allow() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(r#"prefix_rule(pattern=["echo"], decision="allow")"#.to_string()),
            command: vec!["echo".to_string(), "safe".to_string()],
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

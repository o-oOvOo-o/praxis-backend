use super::*;

#[tokio::test]
async fn justification_is_included_in_forbidden_exec_approval_requirement() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(
                r#"
prefix_rule(
    pattern=["rm"],
    decision="forbidden",
    justification="destructive command",
)
"#
                .to_string(),
            ),
            command: vec![
                "rm".to_string(),
                "-rf".to_string(),
                "/some/important/folder".to_string(),
            ],
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Forbidden {
            reason: "`rm -rf /some/important/folder` rejected: destructive command".to_string(),
        },
    )
    .await;
}

#[tokio::test]
async fn exec_approval_requirement_prefers_execpolicy_match() {
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
async fn absolute_path_exec_approval_requirement_matches_host_executable_rules() {
    let git_path = host_program_path("git");
    let git_path_literal = starlark_string(&git_path);
    let policy_src = format!(
        r#"
host_executable(name = "git", paths = ["{git_path_literal}"])
prefix_rule(pattern=["git"], decision="allow")
"#
    );
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(policy_src),
            command: vec![git_path, "status".to_string()],
            approval_policy: AskForApproval::UnlessTrusted,
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
async fn absolute_path_exec_approval_requirement_ignores_disallowed_host_executable_paths() {
    let allowed_git_path = host_program_path("git");
    let disallowed_git_path = host_absolute_path(&[
        "opt",
        "homebrew",
        "bin",
        if cfg!(windows) { "git.exe" } else { "git" },
    ]);
    let allowed_git_path_literal = starlark_string(&allowed_git_path);
    let policy_src = format!(
        r#"
host_executable(name = "git", paths = ["{allowed_git_path_literal}"])
prefix_rule(pattern=["git"], decision="prompt")
"#
    );
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(policy_src),
            command: vec![disallowed_git_path.clone(), "status".to_string()],
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                disallowed_git_path,
                "status".to_string(),
            ])),
        },
    )
    .await;
}

#[tokio::test]
async fn requested_prefix_rule_can_approve_absolute_path_commands() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: vec![
                host_program_path("cargo"),
                "install".to_string(),
                "cargo-insta".to_string(),
            ],
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: Some(vec!["cargo".to_string(), "install".to_string()]),
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                "cargo".to_string(),
                "install".to_string(),
            ])),
        },
    )
    .await;
}

#[tokio::test]
async fn exec_approval_requirement_respects_approval_policy() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: Some(r#"prefix_rule(pattern=["rm"], decision="prompt")"#.to_string()),
            command: vec!["rm".to_string()],
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Forbidden {
            reason: PROMPT_CONFLICT_REASON.to_string(),
        },
    )
    .await;
}

#[test]
fn unmatched_granular_policy_still_prompts_for_restricted_sandbox_escalation() {
    let command = vec!["madeup-cmd".to_string()];

    assert_eq!(
        Decision::Prompt,
        render_decision_for_unmatched_command(
            AskForApproval::Granular(GranularApprovalConfig {
                sandbox_approval: true,
                rules: true,
                skill_approval: true,
                request_permissions: true,
                mcp_elicitations: true,
            }),
            &SandboxPolicy::new_read_only_policy(),
            &read_only_file_system_sandbox_policy(),
            &command,
            SandboxPermissions::RequireEscalated,
            /*used_complex_parsing*/ false,
        )
    );
}

#[test]
fn unmatched_on_request_uses_split_filesystem_policy_for_escalation_prompts() {
    let command = vec!["madeup-cmd".to_string()];
    let restricted_file_system_policy = FileSystemSandboxPolicy::restricted(vec![]);

    assert_eq!(
        Decision::Prompt,
        render_decision_for_unmatched_command(
            AskForApproval::OnRequest,
            &SandboxPolicy::DangerFullAccess,
            &restricted_file_system_policy,
            &command,
            SandboxPermissions::RequireEscalated,
            /*used_complex_parsing*/ false,
        )
    );
}

#[tokio::test]
async fn exec_approval_requirement_prompts_for_inline_additional_permissions_under_on_request() {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: vec![
                "zsh".to_string(),
                "-lc".to_string(),
                "touch requested-dir/requested-but-unused.txt".to_string(),
            ],
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::WithAdditionalPermissions,
            prefix_rule: None,
        },
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                "touch".to_string(),
                "requested-dir/requested-but-unused.txt".to_string(),
            ])),
        },
    )
    .await;
}

#[tokio::test]
async fn exec_approval_requirement_rejects_unmatched_sandbox_escalation_when_granular_sandbox_is_disabled()
 {
    assert_exec_approval_requirement_for_command(
        ExecApprovalRequirementScenario {
            policy_src: None,
            command: vec!["madeup-cmd".to_string()],
            approval_policy: AskForApproval::Granular(GranularApprovalConfig {
                sandbox_approval: false,
                rules: true,
                skill_approval: true,
                request_permissions: true,
                mcp_elicitations: true,
            }),
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            prefix_rule: None,
        },
        ExecApprovalRequirement::Forbidden {
            reason: REJECT_SANDBOX_APPROVAL_REASON.to_string(),
        },
    )
    .await;
}

#[tokio::test]
async fn mixed_rule_and_sandbox_prompt_prioritizes_rule_for_rejection_decision() {
    let policy_src = r#"prefix_rule(pattern=["git"], decision="prompt")"#;
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", policy_src)
        .expect("parse policy");
    let manager = ExecPolicyManager::new(Arc::new(parser.build()));
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "git status && madeup-cmd".to_string(),
    ];

    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::Granular(GranularApprovalConfig {
                sandbox_approval: true,
                rules: true,
                skill_approval: true,
                request_permissions: true,
                mcp_elicitations: true,
            }),
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            prefix_rule: None,
        })
        .await;

    assert!(matches!(
        requirement,
        ExecApprovalRequirement::NeedsApproval { .. }
    ));
}

#[tokio::test]
async fn mixed_rule_and_sandbox_prompt_rejects_when_granular_rules_are_disabled() {
    let policy_src = r#"prefix_rule(pattern=["git"], decision="prompt")"#;
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", policy_src)
        .expect("parse policy");
    let manager = ExecPolicyManager::new(Arc::new(parser.build()));
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "git status && madeup-cmd".to_string(),
    ];

    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::Granular(GranularApprovalConfig {
                sandbox_approval: true,
                rules: false,
                skill_approval: true,
                request_permissions: true,
                mcp_elicitations: true,
            }),
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::Forbidden {
            reason: REJECT_RULES_APPROVAL_REASON.to_string(),
        }
    );
}

#[tokio::test]
async fn exec_approval_requirement_falls_back_to_heuristics() {
    let command = vec!["cargo".to_string(), "build".to_string()];

    let manager = ExecPolicyManager::default();
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command))
        }
    );
}

#[tokio::test]
async fn empty_bash_lc_script_falls_back_to_original_command() {
    let command = vec!["bash".to_string(), "-lc".to_string(), "".to_string()];

    let manager = ExecPolicyManager::default();
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command)),
        }
    );
}

#[tokio::test]
async fn whitespace_bash_lc_script_falls_back_to_original_command() {
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "  \n\t  ".to_string(),
    ];

    let manager = ExecPolicyManager::default();
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command)),
        }
    );
}

#[tokio::test]
async fn request_rule_uses_prefix_rule() {
    let command = vec![
        "cargo".to_string(),
        "install".to_string(),
        "cargo-insta".to_string(),
    ];
    let manager = ExecPolicyManager::default();

    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            prefix_rule: Some(vec!["cargo".to_string(), "install".to_string()]),
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                "cargo".to_string(),
                "install".to_string(),
            ])),
        }
    );
}

#[tokio::test]
async fn request_rule_falls_back_when_prefix_rule_does_not_approve_all_commands() {
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "cargo install cargo-insta && rm -rf /tmp/praxis".to_string(),
    ];
    let manager = ExecPolicyManager::default();

    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: &SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: &unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::RequireEscalated,
            prefix_rule: Some(vec!["cargo".to_string(), "install".to_string()]),
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                "rm".to_string(),
                "-rf".to_string(),
                "/tmp/praxis".to_string(),
            ])),
        }
    );
}

#[tokio::test]
async fn heuristics_apply_when_other_commands_match_policy() {
    let policy_src = r#"prefix_rule(pattern=["apple"], decision="allow")"#;
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", policy_src)
        .expect("parse policy");
    let policy = Arc::new(parser.build());
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "apple | orange".to_string(),
    ];

    assert_eq!(
        ExecPolicyManager::new(policy)
            .create_exec_approval_requirement_for_command(ExecApprovalRequest {
                command: &command,
                approval_policy: AskForApproval::UnlessTrusted,
                sandbox_policy: &SandboxPolicy::DangerFullAccess,
                file_system_sandbox_policy: &unrestricted_file_system_sandbox_policy(),
                sandbox_permissions: SandboxPermissions::UseDefault,
                prefix_rule: None,
            })
            .await,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
                "orange".to_string()
            ]))
        }
    );
}

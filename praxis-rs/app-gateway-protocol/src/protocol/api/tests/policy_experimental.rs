use super::*;

#[test]
fn sandbox_policy_round_trips_external_sandbox_network_access() {
    let api_policy = SandboxPolicy::ExternalSandbox {
        network_access: NetworkAccess::Enabled,
    };

    let core_policy = api_policy.to_core();
    assert_eq!(
        core_policy,
        praxis_protocol::protocol::SandboxPolicy::ExternalSandbox {
            network_access: CoreNetworkAccess::Enabled,
        }
    );

    let back_to_api = SandboxPolicy::from(core_policy);
    assert_eq!(back_to_api, api_policy);
}

#[test]
fn sandbox_policy_round_trips_read_only_access() {
    let readable_root = test_absolute_path();
    let api_policy = SandboxPolicy::ReadOnly {
        access: ReadOnlyAccess::Restricted {
            include_platform_defaults: false,
            readable_roots: vec![readable_root.clone()],
        },
        network_access: true,
    };

    let core_policy = api_policy.to_core();
    assert_eq!(
        core_policy,
        praxis_protocol::protocol::SandboxPolicy::ReadOnly {
            access: CoreReadOnlyAccess::Restricted {
                include_platform_defaults: false,
                readable_roots: vec![readable_root],
            },
            network_access: true,
        }
    );

    let back_to_api = SandboxPolicy::from(core_policy);
    assert_eq!(back_to_api, api_policy);
}

#[test]
fn ask_for_approval_granular_round_trips_request_permissions_flag() {
    let api_policy = AskForApproval::Granular {
        sandbox_approval: true,
        rules: false,
        skill_approval: false,
        request_permissions: true,
        mcp_elicitations: false,
    };

    let core_policy = api_policy.to_core();
    assert_eq!(
        core_policy,
        CoreAskForApproval::Granular(CoreGranularApprovalConfig {
            sandbox_approval: true,
            rules: false,
            skill_approval: false,
            request_permissions: true,
            mcp_elicitations: false,
        })
    );

    let back_to_api = AskForApproval::from(core_policy);
    assert_eq!(back_to_api, api_policy);
}

#[test]
fn ask_for_approval_granular_defaults_missing_optional_flags_to_false() {
    let decoded = serde_json::from_value::<AskForApproval>(serde_json::json!({
        "granular": {
            "sandbox_approval": true,
            "rules": false,
            "mcp_elicitations": true,
        }
    }))
    .expect("granular approval policy should deserialize");

    assert_eq!(
        decoded,
        AskForApproval::Granular {
            sandbox_approval: true,
            rules: false,
            skill_approval: false,
            request_permissions: false,
            mcp_elicitations: true,
        }
    );
}

#[test]
fn ask_for_approval_granular_is_marked_experimental() {
    let reason =
        crate::experimental_api::ExperimentalApi::experimental_reason(&AskForApproval::Granular {
            sandbox_approval: true,
            rules: false,
            skill_approval: false,
            request_permissions: false,
            mcp_elicitations: true,
        });

    assert_eq!(reason, Some("askForApproval.granular"));
    assert_eq!(
        crate::experimental_api::ExperimentalApi::experimental_reason(&AskForApproval::OnRequest,),
        None
    );
}

#[test]
fn profile_granular_approval_policy_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&Profile {
        model: None,
        model_provider: None,
        approval_policy: Some(AskForApproval::Granular {
            sandbox_approval: true,
            rules: false,
            skill_approval: false,
            request_permissions: true,
            mcp_elicitations: false,
        }),
        approvals_reviewer: None,
        service_tier: None,
        model_reasoning_effort: None,
        model_reasoning_summary: None,
        model_verbosity: None,
        web_search: None,
        tools: None,
        chatgpt_base_url: None,
        additional: HashMap::new(),
    });

    assert_eq!(reason, Some("askForApproval.granular"));
}

#[test]
fn config_granular_approval_policy_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&Config {
        model: None,
        review_model: None,
        model_context_window: None,
        model_auto_compact_token_limit: None,
        model_provider: None,
        approval_policy: Some(AskForApproval::Granular {
            sandbox_approval: false,
            rules: true,
            skill_approval: false,
            request_permissions: false,
            mcp_elicitations: true,
        }),
        approvals_reviewer: None,
        sandbox_mode: None,
        sandbox_workspace_write: None,
        forced_chatgpt_workspace_id: None,
        forced_login_method: None,
        web_search: None,
        tools: None,
        profile: None,
        profiles: HashMap::new(),
        instructions: None,
        developer_instructions: None,
        compact_prompt: None,
        model_reasoning_effort: None,
        model_reasoning_summary: None,
        model_verbosity: None,
        service_tier: None,
        analytics: None,
        apps: None,
        additional: HashMap::new(),
    });

    assert_eq!(reason, Some("askForApproval.granular"));
}

#[test]
fn config_approvals_reviewer_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&Config {
        model: None,
        review_model: None,
        model_context_window: None,
        model_auto_compact_token_limit: None,
        model_provider: None,
        approval_policy: None,
        approvals_reviewer: Some(ApprovalsReviewer::GuardianSubagent),
        sandbox_mode: None,
        sandbox_workspace_write: None,
        forced_chatgpt_workspace_id: None,
        forced_login_method: None,
        web_search: None,
        tools: None,
        profile: None,
        profiles: HashMap::new(),
        instructions: None,
        developer_instructions: None,
        compact_prompt: None,
        model_reasoning_effort: None,
        model_reasoning_summary: None,
        model_verbosity: None,
        service_tier: None,
        analytics: None,
        apps: None,
        additional: HashMap::new(),
    });

    assert_eq!(reason, Some("config/read.approvalsReviewer"));
}

#[test]
fn config_nested_profile_granular_approval_policy_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&Config {
        model: None,
        review_model: None,
        model_context_window: None,
        model_auto_compact_token_limit: None,
        model_provider: None,
        approval_policy: None,
        approvals_reviewer: None,
        sandbox_mode: None,
        sandbox_workspace_write: None,
        forced_chatgpt_workspace_id: None,
        forced_login_method: None,
        web_search: None,
        tools: None,
        profile: None,
        profiles: HashMap::from([(
            "default".to_string(),
            Profile {
                model: None,
                model_provider: None,
                approval_policy: Some(AskForApproval::Granular {
                    sandbox_approval: true,
                    rules: false,
                    skill_approval: false,
                    request_permissions: false,
                    mcp_elicitations: true,
                }),
                approvals_reviewer: None,
                service_tier: None,
                model_reasoning_effort: None,
                model_reasoning_summary: None,
                model_verbosity: None,
                web_search: None,
                tools: None,
                chatgpt_base_url: None,
                additional: HashMap::new(),
            },
        )]),
        instructions: None,
        developer_instructions: None,
        compact_prompt: None,
        model_reasoning_effort: None,
        model_reasoning_summary: None,
        model_verbosity: None,
        service_tier: None,
        analytics: None,
        apps: None,
        additional: HashMap::new(),
    });

    assert_eq!(reason, Some("askForApproval.granular"));
}

#[test]
fn config_nested_profile_approvals_reviewer_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&Config {
        model: None,
        review_model: None,
        model_context_window: None,
        model_auto_compact_token_limit: None,
        model_provider: None,
        approval_policy: None,
        approvals_reviewer: None,
        sandbox_mode: None,
        sandbox_workspace_write: None,
        forced_chatgpt_workspace_id: None,
        forced_login_method: None,
        web_search: None,
        tools: None,
        profile: None,
        profiles: HashMap::from([(
            "default".to_string(),
            Profile {
                model: None,
                model_provider: None,
                approval_policy: None,
                approvals_reviewer: Some(ApprovalsReviewer::GuardianSubagent),
                service_tier: None,
                model_reasoning_effort: None,
                model_reasoning_summary: None,
                model_verbosity: None,
                web_search: None,
                tools: None,
                chatgpt_base_url: None,
                additional: HashMap::new(),
            },
        )]),
        instructions: None,
        developer_instructions: None,
        compact_prompt: None,
        model_reasoning_effort: None,
        model_reasoning_summary: None,
        model_verbosity: None,
        service_tier: None,
        analytics: None,
        apps: None,
        additional: HashMap::new(),
    });

    assert_eq!(reason, Some("config/read.approvalsReviewer"));
}

#[test]
fn config_requirements_granular_allowed_approval_policy_is_marked_experimental() {
    let reason =
        crate::experimental_api::ExperimentalApi::experimental_reason(&ConfigRequirements {
            allowed_approval_policies: Some(vec![AskForApproval::Granular {
                sandbox_approval: true,
                rules: true,
                skill_approval: false,
                request_permissions: false,
                mcp_elicitations: false,
            }]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            feature_requirements: None,
            enforce_residency: None,
            network: None,
        });

    assert_eq!(reason, Some("askForApproval.granular"));
}

#[test]
fn client_request_thread_start_granular_approval_policy_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(
        &crate::ClientRequest::ThreadStart {
            request_id: crate::RequestId::Integer(1),
            params: ThreadStartParams {
                approval_policy: Some(AskForApproval::Granular {
                    sandbox_approval: true,
                    rules: false,
                    skill_approval: false,
                    request_permissions: true,
                    mcp_elicitations: false,
                }),
                ..Default::default()
            },
        },
    );

    assert_eq!(reason, Some("askForApproval.granular"));
}

#[test]
fn client_request_thread_resume_granular_approval_policy_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(
        &crate::ClientRequest::ThreadResume {
            request_id: crate::RequestId::Integer(2),
            params: ThreadResumeParams {
                thread_id: "thr_123".to_string(),
                approval_policy: Some(AskForApproval::Granular {
                    sandbox_approval: false,
                    rules: true,
                    skill_approval: false,
                    request_permissions: false,
                    mcp_elicitations: true,
                }),
                ..Default::default()
            },
        },
    );

    assert_eq!(reason, Some("askForApproval.granular"));
}

#[test]
fn client_request_thread_fork_granular_approval_policy_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(
        &crate::ClientRequest::ThreadFork {
            request_id: crate::RequestId::Integer(3),
            params: ThreadForkParams {
                thread_id: "thr_456".to_string(),
                approval_policy: Some(AskForApproval::Granular {
                    sandbox_approval: true,
                    rules: false,
                    skill_approval: false,
                    request_permissions: false,
                    mcp_elicitations: true,
                }),
                ..Default::default()
            },
        },
    );

    assert_eq!(reason, Some("askForApproval.granular"));
}

#[test]
fn client_request_turn_start_granular_approval_policy_is_marked_experimental() {
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(
        &crate::ClientRequest::TurnStart {
            request_id: crate::RequestId::Integer(4),
            params: TurnStartParams {
                thread_id: "thr_123".to_string(),
                input: Vec::new(),
                approval_policy: Some(AskForApproval::Granular {
                    sandbox_approval: false,
                    rules: true,
                    skill_approval: false,
                    request_permissions: false,
                    mcp_elicitations: true,
                }),
                ..Default::default()
            },
        },
    );

    assert_eq!(reason, Some("askForApproval.granular"));
}

use super::*;

#[tokio::test]
async fn refresh_pending_thread_approvals_only_lists_inactive_threads() {
    let mut app = make_test_app().await;
    let main_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000001").expect("valid thread");
    let agent_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000002").expect("valid thread");

    app.primary_thread_id = Some(main_thread_id);
    app.active_thread_id = Some(main_thread_id);
    app.thread_event_channels
        .insert(main_thread_id, ThreadEventChannel::new(/*capacity*/ 1));

    let agent_channel = ThreadEventChannel::new(/*capacity*/ 1);
    {
        let mut store = agent_channel.store.lock().await;
        store.push_request(exec_approval_request(
            agent_thread_id,
            "turn-1",
            "call-1",
            /*approval_id*/ None,
        ));
    }
    app.thread_event_channels
        .insert(agent_thread_id, agent_channel);
    app.agent_navigation.upsert(
        agent_thread_id,
        Some("墨子".to_string()),
        Some("审批工具".to_string()),
        Some("Robie".to_string()),
        Some("explorer".to_string()),
        /*is_closed*/ false,
    );

    app.refresh_pending_thread_approvals().await;
    assert_eq!(
        app.chat_widget.pending_thread_approvals(),
        &["墨子-审批工具 [explorer]".to_string()]
    );

    app.active_thread_id = Some(agent_thread_id);
    app.refresh_pending_thread_approvals().await;
    assert!(app.chat_widget.pending_thread_approvals().is_empty());
}

#[tokio::test]
async fn inactive_thread_approval_bubbles_into_active_view() -> Result<()> {
    let mut app = make_test_app().await;
    let main_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000011").expect("valid thread");
    let agent_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000022").expect("valid thread");

    app.primary_thread_id = Some(main_thread_id);
    app.active_thread_id = Some(main_thread_id);
    app.thread_event_channels
        .insert(main_thread_id, ThreadEventChannel::new(/*capacity*/ 1));
    app.thread_event_channels.insert(
        agent_thread_id,
        ThreadEventChannel::new_with_session(
            /*capacity*/ 1,
            ThreadSessionState {
                approval_policy: AskForApproval::OnRequest,
                sandbox_policy: SandboxPolicy::new_workspace_write_policy(),
                rollout_path: Some(PathBuf::from("/tmp/agent-rollout.jsonl")),
                ..test_thread_session(agent_thread_id, PathBuf::from("/tmp/agent"))
            },
            Vec::new(),
        ),
    );
    app.agent_navigation.upsert(
        agent_thread_id,
        Some("墨子".to_string()),
        Some("审批工具".to_string()),
        Some("Robie".to_string()),
        Some("explorer".to_string()),
        /*is_closed*/ false,
    );

    app.enqueue_thread_request(
        agent_thread_id,
        exec_approval_request(
            agent_thread_id,
            "turn-approval",
            "call-approval",
            /*approval_id*/ None,
        ),
    )
    .await?;

    assert_eq!(app.chat_widget.has_active_view(), true);
    assert_eq!(
        app.chat_widget.pending_thread_approvals(),
        &["墨子-审批工具 [explorer]".to_string()]
    );

    Ok(())
}

#[tokio::test]
async fn inactive_thread_exec_approval_preserves_context() {
    let app = make_test_app().await;
    let thread_id = ThreadId::new();
    let mut request = exec_approval_request(
        thread_id,
        "turn-approval",
        "call-approval",
        /*approval_id*/ None,
    );
    let ServerRequest::CommandExecutionRequestApproval { params, .. } = &mut request else {
        panic!("expected exec approval request");
    };
    params.network_approval_context = Some(AppGatewayNetworkApprovalContext {
        host: "example.com".to_string(),
        protocol: AppGatewayNetworkApprovalProtocol::Socks5Tcp,
    });
    params.additional_permissions = Some(AdditionalPermissionProfile {
        network: Some(AdditionalNetworkPermissions {
            enabled: Some(true),
        }),
        file_system: Some(AdditionalFileSystemPermissions {
            read: Some(vec![test_absolute_path("/tmp/read-only")]),
            write: Some(vec![test_absolute_path("/tmp/write")]),
        }),
    });
    params.proposed_network_policy_amendments = Some(vec![AppGatewayNetworkPolicyAmendment {
        host: "example.com".to_string(),
        action: AppGatewayNetworkPolicyRuleAction::Allow,
    }]);

    let Some(ThreadInteractiveRequest::Approval(ApprovalRequest::Exec {
        available_decisions,
        network_approval_context,
        additional_permissions,
        ..
    })) = app
        .interactive_request_for_thread_request(thread_id, &request)
        .await
    else {
        panic!("expected exec approval request");
    };

    assert_eq!(
        network_approval_context,
        Some(NetworkApprovalContext {
            host: "example.com".to_string(),
            protocol: NetworkApprovalProtocol::Socks5Tcp,
        })
    );
    assert_eq!(
        additional_permissions,
        Some(PermissionProfile {
            network: Some(NetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(FileSystemPermissions {
                read: Some(vec![test_absolute_path("/tmp/read-only")]),
                write: Some(vec![test_absolute_path("/tmp/write")]),
            }),
        })
    );
    assert_eq!(
        available_decisions,
        vec![
            praxis_protocol::protocol::ReviewDecision::Approved,
            praxis_protocol::protocol::ReviewDecision::ApprovedForSession,
            praxis_protocol::protocol::ReviewDecision::NetworkPolicyAmendment {
                network_policy_amendment: praxis_protocol::approvals::NetworkPolicyAmendment {
                    host: "example.com".to_string(),
                    action: praxis_protocol::approvals::NetworkPolicyRuleAction::Allow,
                },
            },
            praxis_protocol::protocol::ReviewDecision::Abort,
        ]
    );
}

#[tokio::test]
async fn inactive_thread_exec_approval_splits_shell_wrapped_command() {
    let app = make_test_app().await;
    let thread_id = ThreadId::new();
    let script = r#"python3 -c 'print("Hello, world!")'"#;
    let mut request = exec_approval_request(
        thread_id,
        "turn-approval",
        "call-approval",
        /*approval_id*/ None,
    );
    let ServerRequest::CommandExecutionRequestApproval { params, .. } = &mut request else {
        panic!("expected exec approval request");
    };
    params.command =
        Some(shlex::try_join(["/bin/zsh", "-lc", script]).expect("round-trippable shell wrapper"));

    let Some(ThreadInteractiveRequest::Approval(ApprovalRequest::Exec { command, .. })) = app
        .interactive_request_for_thread_request(thread_id, &request)
        .await
    else {
        panic!("expected exec approval request");
    };

    assert_eq!(
        command,
        vec![
            "/bin/zsh".to_string(),
            "-lc".to_string(),
            script.to_string(),
        ]
    );
}

#[tokio::test]
async fn inactive_thread_permissions_approval_preserves_file_system_permissions() {
    let app = make_test_app().await;
    let thread_id = ThreadId::new();
    let request = ServerRequest::PermissionsRequestApproval {
        request_id: AppGatewayRequestId::Integer(7),
        params: PermissionsRequestApprovalParams {
            thread_id: thread_id.to_string(),
            turn_id: "turn-approval".to_string(),
            item_id: "call-approval".to_string(),
            reason: Some("Need access to .git".to_string()),
            permissions: praxis_app_gateway_protocol::RequestPermissionProfile {
                network: Some(AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(AdditionalFileSystemPermissions {
                    read: Some(vec![test_absolute_path("/tmp/read-only")]),
                    write: Some(vec![test_absolute_path("/tmp/write")]),
                }),
            },
        },
    };

    let Some(ThreadInteractiveRequest::Approval(ApprovalRequest::Permissions {
        permissions, ..
    })) = app
        .interactive_request_for_thread_request(thread_id, &request)
        .await
    else {
        panic!("expected permissions approval request");
    };

    assert_eq!(
        permissions,
        RequestPermissionProfile {
            network: Some(NetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(FileSystemPermissions {
                read: Some(vec![test_absolute_path("/tmp/read-only")]),
                write: Some(vec![test_absolute_path("/tmp/write")]),
            }),
        }
    );
}

#[tokio::test]
async fn inactive_thread_approval_badge_clears_after_turn_completion_notification() -> Result<()> {
    let mut app = make_test_app().await;
    let main_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000101").expect("valid thread");
    let agent_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000202").expect("valid thread");

    app.primary_thread_id = Some(main_thread_id);
    app.active_thread_id = Some(main_thread_id);
    app.thread_event_channels
        .insert(main_thread_id, ThreadEventChannel::new(/*capacity*/ 1));
    app.thread_event_channels.insert(
        agent_thread_id,
        ThreadEventChannel::new_with_session(
            /*capacity*/ 4,
            ThreadSessionState {
                approval_policy: AskForApproval::OnRequest,
                sandbox_policy: SandboxPolicy::new_workspace_write_policy(),
                rollout_path: Some(PathBuf::from("/tmp/agent-rollout.jsonl")),
                ..test_thread_session(agent_thread_id, PathBuf::from("/tmp/agent"))
            },
            Vec::new(),
        ),
    );
    app.agent_navigation.upsert(
        agent_thread_id,
        Some("墨子".to_string()),
        Some("审批工具".to_string()),
        Some("Robie".to_string()),
        Some("explorer".to_string()),
        /*is_closed*/ false,
    );

    app.enqueue_thread_request(
        agent_thread_id,
        exec_approval_request(
            agent_thread_id,
            "turn-approval",
            "call-approval",
            /*approval_id*/ None,
        ),
    )
    .await?;
    assert_eq!(
        app.chat_widget.pending_thread_approvals(),
        &["Robie [explorer]".to_string()]
    );

    app.enqueue_thread_notification(
        agent_thread_id,
        turn_completed_notification(agent_thread_id, "turn-approval", TurnStatus::Completed),
    )
    .await?;

    assert!(
        app.chat_widget.pending_thread_approvals().is_empty(),
        "turn completion should clear inactive-thread approval badge immediately"
    );

    Ok(())
}

#[tokio::test]
async fn inactive_thread_started_notification_initializes_replay_session() -> Result<()> {
    let mut app = make_test_app().await;
    let temp_dir = tempdir()?;
    let main_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000101").expect("valid thread");
    let agent_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000202").expect("valid thread");
    let primary_session = ThreadSessionState {
        approval_policy: AskForApproval::OnRequest,
        sandbox_policy: SandboxPolicy::new_workspace_write_policy(),
        ..test_thread_session(main_thread_id, PathBuf::from("/tmp/main"))
    };

    app.primary_thread_id = Some(main_thread_id);
    app.active_thread_id = Some(main_thread_id);
    app.primary_session_configured = Some(primary_session.clone());
    app.thread_event_channels.insert(
        main_thread_id,
        ThreadEventChannel::new_with_session(
            /*capacity*/ 4,
            primary_session.clone(),
            Vec::new(),
        ),
    );

    let rollout_path = temp_dir.path().join("agent-rollout.jsonl");
    let turn_context = TurnContextItem {
        turn_id: None,
        trace_id: None,
        cwd: PathBuf::from("/tmp/agent"),
        current_date: None,
        timezone: None,
        approval_policy: primary_session.approval_policy,
        sandbox_policy: primary_session.sandbox_policy.clone(),
        network: None,
        model: "gpt-agent".to_string(),
        personality: None,
        collaboration_mode: None,
        realtime_active: Some(false),
        effort: primary_session.reasoning_effort.clone(),
        summary: app.config.model_reasoning_summary.unwrap_or_default(),
        user_instructions: None,
        developer_instructions: None,
        final_output_json_schema: None,
        truncation_policy: None,
    };
    let rollout = RolloutLine {
        timestamp: "t0".to_string(),
        item: RolloutItem::TurnContext(turn_context),
    };
    std::fs::write(
        &rollout_path,
        format!("{}\n", serde_json::to_string(&rollout)?),
    )?;
    app.enqueue_thread_notification(
        agent_thread_id,
        ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: Thread {
                id: agent_thread_id.to_string(),
                preview: "agent thread".to_string(),
                summary: None,
                ephemeral: false,
                model_provider: "agent-provider".to_string(),
                model: Some("gpt-agent".to_string()),
                created_at: 1,
                updated_at: 2,
                status: praxis_app_gateway_protocol::ThreadStatus::Idle,
                path: Some(rollout_path.clone()),
                cwd: PathBuf::from("/tmp/agent"),
                cli_version: "0.0.0".to_string(),
                source: praxis_app_gateway_protocol::SessionSource::Unknown,
                agent_base_name: Some("墨子".to_string()),
                agent_title: Some("巡检仓库".to_string()),
                agent_display_name: Some("Robie".to_string()),
                agent_role: Some("explorer".to_string()),
                git_info: None,
                name: Some("agent thread".to_string()),
                total_cost_usd: None,
                last_cost_usd: None,
                token_usage: None,
                control_state: None,
                selfwork_plan_path: None,
                turns: Vec::new(),
            },
        }),
    )
    .await?;

    let store = app
        .thread_event_channels
        .get(&agent_thread_id)
        .expect("agent thread channel")
        .store
        .lock()
        .await;
    let session = store.session.clone().expect("inferred session");
    drop(store);

    assert_eq!(session.thread_id, agent_thread_id);
    assert_eq!(session.thread_name, Some("agent thread".to_string()));
    assert_eq!(session.model, "gpt-agent");
    assert_eq!(session.model_provider_id, "agent-provider");
    assert_eq!(session.approval_policy, primary_session.approval_policy);
    assert_eq!(session.cwd, PathBuf::from("/tmp/agent"));
    assert_eq!(session.rollout_path, Some(rollout_path));
    assert_eq!(
        app.agent_navigation.get(&agent_thread_id),
        Some(&AgentPickerThreadEntry {
            agent_base_name: Some("墨子".to_string()),
            agent_title: Some("巡检仓库".to_string()),
            agent_display_name: Some("Robie".to_string()),
            agent_role: Some("explorer".to_string()),
            is_closed: false,
        })
    );

    Ok(())
}

#[tokio::test]
async fn inactive_thread_started_notification_preserves_primary_model_when_path_missing()
-> Result<()> {
    let mut app = make_test_app().await;
    let main_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000301").expect("valid thread");
    let agent_thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000302").expect("valid thread");
    let primary_session = ThreadSessionState {
        approval_policy: AskForApproval::OnRequest,
        sandbox_policy: SandboxPolicy::new_workspace_write_policy(),
        ..test_thread_session(main_thread_id, PathBuf::from("/tmp/main"))
    };

    app.primary_thread_id = Some(main_thread_id);
    app.active_thread_id = Some(main_thread_id);
    app.primary_session_configured = Some(primary_session.clone());
    app.thread_event_channels.insert(
        main_thread_id,
        ThreadEventChannel::new_with_session(
            /*capacity*/ 4,
            primary_session.clone(),
            Vec::new(),
        ),
    );

    app.enqueue_thread_notification(
        agent_thread_id,
        ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: Thread {
                id: agent_thread_id.to_string(),
                preview: "agent thread".to_string(),
                summary: None,
                ephemeral: false,
                model_provider: "agent-provider".to_string(),
                model: Some("gpt-agent".to_string()),
                created_at: 1,
                updated_at: 2,
                status: praxis_app_gateway_protocol::ThreadStatus::Idle,
                path: None,
                cwd: PathBuf::from("/tmp/agent"),
                cli_version: "0.0.0".to_string(),
                source: praxis_app_gateway_protocol::SessionSource::Unknown,
                agent_base_name: Some("墨子".to_string()),
                agent_title: Some("巡检仓库".to_string()),
                agent_display_name: Some("Robie".to_string()),
                agent_role: Some("explorer".to_string()),
                git_info: None,
                name: Some("agent thread".to_string()),
                total_cost_usd: None,
                last_cost_usd: None,
                token_usage: None,
                control_state: None,
                selfwork_plan_path: None,
                turns: Vec::new(),
            },
        }),
    )
    .await?;

    let store = app
        .thread_event_channels
        .get(&agent_thread_id)
        .expect("agent thread channel")
        .store
        .lock()
        .await;
    let session = store.session.clone().expect("inferred session");

    assert_eq!(session.model, primary_session.model);

    Ok(())
}

#[test]
fn agent_picker_item_name_snapshot() {
    use crate::multi_agents::format_agent_picker_item_name;

    let thread_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000123").expect("valid thread id");
    let snapshot = [
        format!(
            "{} | {}",
            format_agent_picker_item_name(
                Some("墨子"),
                Some("修复碰撞"),
                Some("Robie"),
                Some("explorer"),
                /*is_primary*/ true
            ),
            thread_id
        ),
        format!(
            "{} | {}",
            format_agent_picker_item_name(
                Some("墨子"),
                Some("修复碰撞"),
                Some("Robie"),
                Some("explorer"),
                /*is_primary*/ false
            ),
            thread_id
        ),
        format!(
            "{} | {}",
            format_agent_picker_item_name(
                Some("墨子"),
                /*agent_title*/ None,
                Some("Robie"),
                /*agent_role*/ None,
                /*is_primary*/ false
            ),
            thread_id
        ),
        format!(
            "{} | {}",
            format_agent_picker_item_name(
                /*agent_base_name*/ None,
                /*agent_title*/ None,
                /*agent_display_name*/ None,
                Some("explorer"),
                /*is_primary*/ false
            ),
            thread_id
        ),
        format!(
            "{} | {}",
            format_agent_picker_item_name(
                /*agent_base_name*/ None, /*agent_title*/ None,
                /*agent_display_name*/ None, /*agent_role*/ None,
                /*is_primary*/ false
            ),
            thread_id
        ),
    ]
    .join("\n");
    assert_snapshot!("agent_picker_item_name", snapshot);
}

#[tokio::test]
async fn active_non_primary_shutdown_target_returns_none_for_non_shutdown_event() -> Result<()> {
    let mut app = make_test_app().await;
    app.active_thread_id = Some(ThreadId::new());
    app.primary_thread_id = Some(ThreadId::new());

    assert_eq!(
        app.active_non_primary_shutdown_target(&ServerNotification::SkillsChanged(
            praxis_app_gateway_protocol::SkillsChangedNotification {},
        )),
        None
    );
    Ok(())
}

#[tokio::test]
async fn active_non_primary_shutdown_target_returns_none_for_primary_thread_shutdown() -> Result<()>
{
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    app.active_thread_id = Some(thread_id);
    app.primary_thread_id = Some(thread_id);

    assert_eq!(
        app.active_non_primary_shutdown_target(&thread_closed_notification(thread_id)),
        None
    );
    Ok(())
}

#[tokio::test]
async fn active_non_primary_shutdown_target_returns_ids_for_non_primary_shutdown() -> Result<()> {
    let mut app = make_test_app().await;
    let active_thread_id = ThreadId::new();
    let primary_thread_id = ThreadId::new();
    app.active_thread_id = Some(active_thread_id);
    app.primary_thread_id = Some(primary_thread_id);

    assert_eq!(
        app.active_non_primary_shutdown_target(&thread_closed_notification(active_thread_id)),
        Some((active_thread_id, primary_thread_id))
    );
    Ok(())
}

#[tokio::test]
async fn active_non_primary_shutdown_target_returns_none_when_shutdown_exit_is_pending()
-> Result<()> {
    let mut app = make_test_app().await;
    let active_thread_id = ThreadId::new();
    let primary_thread_id = ThreadId::new();
    app.active_thread_id = Some(active_thread_id);
    app.primary_thread_id = Some(primary_thread_id);
    app.pending_shutdown_exit_thread_id = Some(active_thread_id);

    assert_eq!(
        app.active_non_primary_shutdown_target(&thread_closed_notification(active_thread_id)),
        None
    );
    Ok(())
}

#[tokio::test]
async fn active_non_primary_shutdown_target_still_switches_for_other_pending_exit_thread()
-> Result<()> {
    let mut app = make_test_app().await;
    let active_thread_id = ThreadId::new();
    let primary_thread_id = ThreadId::new();
    app.active_thread_id = Some(active_thread_id);
    app.primary_thread_id = Some(primary_thread_id);
    app.pending_shutdown_exit_thread_id = Some(ThreadId::new());

    assert_eq!(
        app.active_non_primary_shutdown_target(&thread_closed_notification(active_thread_id)),
        Some((active_thread_id, primary_thread_id))
    );
    Ok(())
}

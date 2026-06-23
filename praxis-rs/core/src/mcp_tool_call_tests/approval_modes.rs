use super::*;

#[tokio::test]
async fn approve_mode_skips_when_annotations_do_not_require_approval() {
    let (session, turn_context) = make_session_and_context().await;
    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context);
    let invocation = McpInvocation {
        server: "custom_server".to_string(),
        tool: "read_only_tool".to_string(),
        arguments: None,
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(
            Some(true),
            /*destructive*/ None,
            /*open_world*/ None,
        )),
        connector_id: None,
        connector_name: None,
        connector_description: None,
        tool_title: Some("Read Only Tool".to_string()),
        tool_description: None,
        praxis_apps_meta: None,
    };

    let decision = maybe_request_mcp_tool_approval(
        &session,
        &turn_context,
        "call-1",
        &invocation,
        Some(&metadata),
        AppToolApproval::Approve,
    )
    .await;

    assert_eq!(decision, None);
}

#[tokio::test]
async fn guardian_mode_skips_auto_when_annotations_do_not_require_approval() {
    use wiremock::Mock;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    let server = start_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let (mut session, mut turn_context) = make_session_and_context().await;
    turn_context
        .approval_policy
        .set(AskForApproval::OnRequest)
        .expect("test setup should allow updating approval policy");
    let mut config = (*turn_context.config).clone();
    config.model_provider.base_url = Some(format!("{}/v1", server.uri()));
    config.approvals_reviewer = ApprovalsReviewer::GuardianSubagent;
    let config = Arc::new(config);
    let models_manager = Arc::new(crate::test_support::models_manager_with_provider(
        config.praxis_home.clone(),
        Arc::clone(&session.services.auth_manager),
        config.model_provider.clone(),
    ));
    session.services.models_manager = models_manager;
    turn_context.config = Arc::clone(&config);
    turn_context.provider = config.model_provider.clone();

    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context);
    let invocation = McpInvocation {
        server: "custom_server".to_string(),
        tool: "read_only_tool".to_string(),
        arguments: None,
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(
            Some(true),
            /*destructive*/ None,
            /*open_world*/ None,
        )),
        connector_id: None,
        connector_name: None,
        connector_description: None,
        tool_title: Some("Read Only Tool".to_string()),
        tool_description: None,
        praxis_apps_meta: None,
    };

    let decision = maybe_request_mcp_tool_approval(
        &session,
        &turn_context,
        "call-guardian",
        &invocation,
        Some(&metadata),
        AppToolApproval::Auto,
    )
    .await;

    assert_eq!(decision, None);
}

#[tokio::test]
async fn prompt_mode_waits_for_approval_when_annotations_do_not_require_approval() {
    let (session, turn_context, _rx_event) = make_session_and_context_with_rx().await;
    {
        let mut active_turn = session.active_turn.lock().await;
        *active_turn = Some(ActiveTurn::default());
    }
    let invocation = McpInvocation {
        server: "custom_server".to_string(),
        tool: "read_only_tool".to_string(),
        arguments: None,
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(
            Some(true),
            /*destructive*/ None,
            /*open_world*/ None,
        )),
        connector_id: None,
        connector_name: None,
        connector_description: None,
        tool_title: Some("Read Only Tool".to_string()),
        tool_description: None,
        praxis_apps_meta: None,
    };

    let mut approval_task = {
        let session = Arc::clone(&session);
        let turn_context = Arc::clone(&turn_context);
        tokio::spawn(async move {
            maybe_request_mcp_tool_approval(
                &session,
                &turn_context,
                "call-prompt",
                &invocation,
                Some(&metadata),
                AppToolApproval::Prompt,
            )
            .await
        })
    };

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), &mut approval_task)
            .await
            .is_err(),
        "prompt mode should wait for approval instead of auto-allowing"
    );
    approval_task.abort();
}

#[tokio::test]
async fn approve_mode_blocks_when_arc_returns_interrupt_for_model() {
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/codex/safety/arc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "outcome": "steer-model",
            "short_reason": "needs approval",
            "rationale": "high-risk action",
            "risk_score": 96,
            "risk_level": "critical",
            "evidence": [{
                "message": "dangerous_tool",
                "why": "high-risk action",
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (session, mut turn_context) = make_session_and_context().await;
    turn_context.auth_manager = Some(crate::test_support::auth_manager_from_auth(
        praxis_login::OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing(),
    ));
    let mut config = (*turn_context.config).clone();
    config.chatgpt_base_url = server.uri();
    turn_context.config = Arc::new(config);

    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context);
    let invocation = McpInvocation {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        tool: "dangerous_tool".to_string(),
        arguments: Some(serde_json::json!({ "id": 1 })),
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(Some(false), Some(true), Some(true))),
        connector_id: Some("calendar".to_string()),
        connector_name: Some("Calendar".to_string()),
        connector_description: Some("Manage events".to_string()),
        tool_title: Some("Dangerous Tool".to_string()),
        tool_description: Some("Performs a risky action.".to_string()),
        praxis_apps_meta: None,
    };

    let decision = maybe_request_mcp_tool_approval(
        &session,
        &turn_context,
        "call-2",
        &invocation,
        Some(&metadata),
        AppToolApproval::Approve,
    )
    .await;

    assert_eq!(
        decision,
        Some(McpToolApprovalDecision::BlockedBySafetyMonitor(
            "Tool call was cancelled because of safety risks: high-risk action".to_string(),
        ))
    );
}

#[tokio::test]
async fn custom_approve_mode_blocks_when_arc_returns_interrupt_for_model() {
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/codex/safety/arc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "outcome": "steer-model",
            "short_reason": "needs approval",
            "rationale": "high-risk action",
            "risk_score": 96,
            "risk_level": "critical",
            "evidence": [{
                "message": "dangerous_tool",
                "why": "high-risk action",
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (session, mut turn_context) = make_session_and_context().await;
    turn_context.auth_manager = Some(crate::test_support::auth_manager_from_auth(
        praxis_login::OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing(),
    ));
    let mut config = (*turn_context.config).clone();
    config.chatgpt_base_url = server.uri();
    turn_context.config = Arc::new(config);

    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context);
    let invocation = McpInvocation {
        server: "docs".to_string(),
        tool: "dangerous_tool".to_string(),
        arguments: Some(serde_json::json!({ "id": 1 })),
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(Some(false), Some(true), Some(true))),
        connector_id: None,
        connector_name: None,
        connector_description: None,
        tool_title: Some("Dangerous Tool".to_string()),
        tool_description: Some("Performs a risky action.".to_string()),
        praxis_apps_meta: None,
    };

    let decision = maybe_request_mcp_tool_approval(
        &session,
        &turn_context,
        "call-2-custom",
        &invocation,
        Some(&metadata),
        AppToolApproval::Approve,
    )
    .await;

    assert_eq!(
        decision,
        Some(McpToolApprovalDecision::BlockedBySafetyMonitor(
            "Tool call was cancelled because of safety risks: high-risk action".to_string(),
        ))
    );
}

#[tokio::test]
async fn approve_mode_blocks_when_arc_returns_interrupt_without_annotations() {
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/codex/safety/arc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "outcome": "steer-model",
            "short_reason": "needs approval",
            "rationale": "high-risk action",
            "risk_score": 96,
            "risk_level": "critical",
            "evidence": [{
                "message": "dangerous_tool",
                "why": "high-risk action",
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (session, mut turn_context) = make_session_and_context().await;
    turn_context.auth_manager = Some(crate::test_support::auth_manager_from_auth(
        praxis_login::OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing(),
    ));
    let mut config = (*turn_context.config).clone();
    config.chatgpt_base_url = server.uri();
    turn_context.config = Arc::new(config);

    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context);
    let invocation = McpInvocation {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        tool: "dangerous_tool".to_string(),
        arguments: Some(serde_json::json!({ "id": 1 })),
    };
    let metadata = McpToolApprovalMetadata {
        annotations: None,
        connector_id: Some("calendar".to_string()),
        connector_name: Some("Calendar".to_string()),
        connector_description: Some("Manage events".to_string()),
        tool_title: Some("Dangerous Tool".to_string()),
        tool_description: Some("Performs a risky action.".to_string()),
        praxis_apps_meta: None,
    };

    let decision = maybe_request_mcp_tool_approval(
        &session,
        &turn_context,
        "call-3",
        &invocation,
        Some(&metadata),
        AppToolApproval::Approve,
    )
    .await;

    assert_eq!(
        decision,
        Some(McpToolApprovalDecision::BlockedBySafetyMonitor(
            "Tool call was cancelled because of safety risks: high-risk action".to_string(),
        ))
    );
}

#[tokio::test]
async fn full_access_mode_skips_arc_monitor_for_all_approval_modes() {
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/codex/safety/arc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "outcome": "steer-model",
            "short_reason": "needs approval",
            "rationale": "high-risk action",
            "risk_score": 96,
            "risk_level": "critical",
            "evidence": [{
                "message": "dangerous_tool",
                "why": "high-risk action",
            }],
        })))
        .expect(0)
        .mount(&server)
        .await;

    let (session, mut turn_context) = make_session_and_context().await;
    turn_context.auth_manager = Some(crate::test_support::auth_manager_from_auth(
        praxis_login::OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing(),
    ));
    turn_context
        .approval_policy
        .set(AskForApproval::Never)
        .expect("test setup should allow updating approval policy");
    turn_context
        .sandbox_policy
        .set(SandboxPolicy::DangerFullAccess)
        .expect("test setup should allow updating sandbox policy");
    let mut config = (*turn_context.config).clone();
    config.chatgpt_base_url = server.uri();
    turn_context.config = Arc::new(config);

    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context);
    let invocation = McpInvocation {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        tool: "dangerous_tool".to_string(),
        arguments: Some(serde_json::json!({ "id": 1 })),
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(Some(false), Some(true), Some(true))),
        connector_id: Some("calendar".to_string()),
        connector_name: Some("Calendar".to_string()),
        connector_description: Some("Manage events".to_string()),
        tool_title: Some("Dangerous Tool".to_string()),
        tool_description: Some("Performs a risky action.".to_string()),
        praxis_apps_meta: None,
    };

    for approval_mode in [
        AppToolApproval::Auto,
        AppToolApproval::Prompt,
        AppToolApproval::Approve,
    ] {
        let decision = maybe_request_mcp_tool_approval(
            &session,
            &turn_context,
            "call-2",
            &invocation,
            Some(&metadata),
            approval_mode,
        )
        .await;

        assert_eq!(decision, None);
    }
}

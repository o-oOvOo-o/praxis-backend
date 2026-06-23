use super::*;

#[tokio::test]
async fn approve_mode_routes_arc_ask_user_to_guardian_when_guardian_reviewer_is_enabled() {
    use wiremock::Mock;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    let server = start_mock_server().await;
    let guardian_request_log = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-guardian"),
            ev_assistant_message(
                "msg-guardian",
                &serde_json::json!({
                    "risk_level": "low",
                    "risk_score": 12,
                    "rationale": "The user already configured guardian to review escalated approvals for this session.",
                    "evidence": [{
                        "message": "ARC requested escalation instead of blocking outright.",
                        "why": "Guardian can adjudicate the approval without surfacing a manual prompt.",
                    }],
                })
                .to_string(),
            ),
            ev_completed("resp-guardian"),
        ]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/codex/safety/arc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "outcome": "ask-user",
            "short_reason": "needs confirmation",
            "rationale": "ARC wants a second review",
            "risk_score": 65,
            "risk_level": "medium",
            "evidence": [{
                "message": "dangerous_tool",
                "why": "requires review",
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (mut session, mut turn_context) = make_session_and_context().await;
    turn_context.auth_manager = Some(crate::test_support::auth_manager_from_auth(
        praxis_login::OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing(),
    ));
    turn_context
        .approval_policy
        .set(AskForApproval::OnRequest)
        .expect("test setup should allow updating approval policy");
    let mut config = (*turn_context.config).clone();
    config.chatgpt_base_url = server.uri();
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
        "call-3",
        &invocation,
        Some(&metadata),
        AppToolApproval::Approve,
    )
    .await;

    assert_eq!(decision, Some(McpToolApprovalDecision::Accept));
    assert_eq!(
        guardian_request_log.single_request().path(),
        "/v1/responses"
    );
}

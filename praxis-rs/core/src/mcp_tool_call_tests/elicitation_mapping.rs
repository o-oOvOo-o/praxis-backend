use super::*;

#[test]
fn accepted_elicitation_content_converts_to_request_user_input_response() {
    let response = request_user_input_response_from_elicitation_content(Some(serde_json::json!(
        {
            "approval": MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER,
        }
    )));

    assert_eq!(
        response,
        Some(RequestUserInputResponse {
            answers: std::collections::HashMap::from([(
                "approval".to_string(),
                RequestUserInputAnswer {
                    answers: vec![MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER.to_string()],
                },
            )]),
        })
    );
}

#[test]
fn approval_elicitation_meta_marks_tool_approvals() {
    assert_eq!(
        build_mcp_tool_approval_elicitation_meta(
            "custom_server",
            /*metadata*/ None,
            /*tool_params*/ None,
            /*tool_params_display*/ None,
            prompt_options(
                /*allow_session_remember*/ false, /*allow_persistent_approval*/ false
            ),
        ),
        Some(serde_json::json!({
            MCP_TOOL_APPROVAL_KIND_KEY: MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL,
        }))
    );
}

#[test]
fn approval_elicitation_meta_merges_session_and_always_persist_for_custom_servers() {
    assert_eq!(
        build_mcp_tool_approval_elicitation_meta(
            "custom_server",
            Some(&approval_metadata(
                /*connector_id*/ None,
                /*connector_name*/ None,
                /*connector_description*/ None,
                Some("Run Action"),
                Some("Runs the selected action."),
            )),
            Some(&serde_json::json!({"id": 1})),
            /*tool_params_display*/ None,
            prompt_options(
                /*allow_session_remember*/ true, /*allow_persistent_approval*/ true
            ),
        ),
        Some(serde_json::json!({
            MCP_TOOL_APPROVAL_KIND_KEY: MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL,
            MCP_TOOL_APPROVAL_PERSIST_KEY: [
                MCP_TOOL_APPROVAL_PERSIST_SESSION,
                MCP_TOOL_APPROVAL_PERSIST_ALWAYS,
            ],
            MCP_TOOL_APPROVAL_TOOL_TITLE_KEY: "Run Action",
            MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY: "Runs the selected action.",
            MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY: {
                "id": 1,
            },
        }))
    );
}

#[test]
fn approval_elicitation_meta_includes_connector_source_for_praxis_apps() {
    assert_eq!(
        build_mcp_tool_approval_elicitation_meta(
            PRAXIS_APPS_MCP_SERVER_NAME,
            Some(&approval_metadata(
                Some("calendar"),
                Some("Calendar"),
                Some("Manage events and schedules."),
                Some("Run Action"),
                Some("Runs the selected action."),
            )),
            Some(&serde_json::json!({
                "calendar_id": "primary",
            })),
            /*tool_params_display*/ None,
            prompt_options(
                /*allow_session_remember*/ false, /*allow_persistent_approval*/ false
            ),
        ),
        Some(serde_json::json!({
            MCP_TOOL_APPROVAL_KIND_KEY: MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL,
            MCP_TOOL_APPROVAL_SOURCE_KEY: MCP_TOOL_APPROVAL_SOURCE_CONNECTOR,
            MCP_TOOL_APPROVAL_CONNECTOR_ID_KEY: "calendar",
            MCP_TOOL_APPROVAL_CONNECTOR_NAME_KEY: "Calendar",
            MCP_TOOL_APPROVAL_CONNECTOR_DESCRIPTION_KEY: "Manage events and schedules.",
            MCP_TOOL_APPROVAL_TOOL_TITLE_KEY: "Run Action",
            MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY: "Runs the selected action.",
            MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY: {
                "calendar_id": "primary",
            },
        }))
    );
}

#[test]
fn approval_elicitation_meta_merges_session_and_always_persist_with_connector_source() {
    assert_eq!(
        build_mcp_tool_approval_elicitation_meta(
            PRAXIS_APPS_MCP_SERVER_NAME,
            Some(&approval_metadata(
                Some("calendar"),
                Some("Calendar"),
                Some("Manage events and schedules."),
                Some("Run Action"),
                Some("Runs the selected action."),
            )),
            Some(&serde_json::json!({
                "calendar_id": "primary",
            })),
            /*tool_params_display*/ None,
            prompt_options(
                /*allow_session_remember*/ true, /*allow_persistent_approval*/ true
            ),
        ),
        Some(serde_json::json!({
            MCP_TOOL_APPROVAL_KIND_KEY: MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL,
            MCP_TOOL_APPROVAL_PERSIST_KEY: [
                MCP_TOOL_APPROVAL_PERSIST_SESSION,
                MCP_TOOL_APPROVAL_PERSIST_ALWAYS,
            ],
            MCP_TOOL_APPROVAL_SOURCE_KEY: MCP_TOOL_APPROVAL_SOURCE_CONNECTOR,
            MCP_TOOL_APPROVAL_CONNECTOR_ID_KEY: "calendar",
            MCP_TOOL_APPROVAL_CONNECTOR_NAME_KEY: "Calendar",
            MCP_TOOL_APPROVAL_CONNECTOR_DESCRIPTION_KEY: "Manage events and schedules.",
            MCP_TOOL_APPROVAL_TOOL_TITLE_KEY: "Run Action",
            MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY: "Runs the selected action.",
            MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY: {
                "calendar_id": "primary",
            },
        }))
    );
}

#[tokio::test]
async fn approval_callsite_mode_distinguishes_default_and_always_allow() {
    let (_session, turn_context) = make_session_and_context().await;

    assert_eq!(
        mcp_tool_approval_callsite_mode(AppToolApproval::Auto, &turn_context),
        "mcp_tool_call__default"
    );
    assert_eq!(
        mcp_tool_approval_callsite_mode(AppToolApproval::Prompt, &turn_context),
        "mcp_tool_call__default"
    );
    assert_eq!(
        mcp_tool_approval_callsite_mode(AppToolApproval::Approve, &turn_context),
        "mcp_tool_call__always_allow"
    );
}

#[test]
fn declined_elicitation_response_stays_decline() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Decline,
            content: Some(serde_json::json!({
                "approval": MCP_TOOL_APPROVAL_ACCEPT,
            })),
            meta: None,
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::Decline);
}

#[test]
fn synthetic_decline_request_user_input_response_stays_decline() {
    let response = parse_mcp_tool_approval_response(
        Some(RequestUserInputResponse {
            answers: HashMap::from([(
                "approval".to_string(),
                RequestUserInputAnswer {
                    answers: vec![MCP_TOOL_APPROVAL_DECLINE_SYNTHETIC.to_string()],
                },
            )]),
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::Decline);
}

#[test]
fn accepted_elicitation_response_uses_always_persist_meta() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Accept,
            content: None,
            meta: Some(serde_json::json!({
                MCP_TOOL_APPROVAL_PERSIST_KEY: MCP_TOOL_APPROVAL_PERSIST_ALWAYS,
            })),
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::AcceptAndRemember);
}

#[test]
fn accepted_elicitation_response_uses_session_persist_meta() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Accept,
            content: None,
            meta: Some(serde_json::json!({
                MCP_TOOL_APPROVAL_PERSIST_KEY: MCP_TOOL_APPROVAL_PERSIST_SESSION,
            })),
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::AcceptForSession);
}

#[test]
fn accepted_elicitation_without_content_defaults_to_accept() {
    let response = parse_mcp_tool_approval_elicitation_response(
        Some(ElicitationResponse {
            action: ElicitationAction::Accept,
            content: None,
            meta: None,
        }),
        "approval",
    );

    assert_eq!(response, McpToolApprovalDecision::Accept);
}

use super::*;

#[tokio::test]
async fn approval_elicitation_request_uses_message_override_and_preserves_tool_params_keys() {
    let (session, turn_context) = make_session_and_context().await;
    let question = build_mcp_tool_approval_question(
        "q".to_string(),
        PRAXIS_APPS_MCP_SERVER_NAME,
        "create_event",
        Some("Calendar"),
        prompt_options(
            /*allow_session_remember*/ true, /*allow_persistent_approval*/ true,
        ),
        Some("Allow Calendar to create an event?"),
    );

    let request = build_mcp_tool_approval_elicitation_request(
        &session,
        &turn_context,
        McpToolApprovalElicitationRequest {
            server: PRAXIS_APPS_MCP_SERVER_NAME,
            metadata: Some(&approval_metadata(
                Some("calendar"),
                Some("Calendar"),
                Some("Manage events and schedules."),
                Some("Create Event"),
                Some("Create a calendar event."),
            )),
            tool_params: Some(&serde_json::json!({
                "calendar_id": "primary",
                "title": "Roadmap review",
            })),
            tool_params_display: Some(&[
                RenderedMcpToolApprovalParam {
                    name: "calendar_id".to_string(),
                    value: serde_json::json!("primary"),
                    display_name: "Calendar".to_string(),
                },
                RenderedMcpToolApprovalParam {
                    name: "title".to_string(),
                    value: serde_json::json!("Roadmap review"),
                    display_name: "Title".to_string(),
                },
            ]),
            question,
            message_override: Some("Allow Calendar to create an event?"),
            prompt_options: prompt_options(
                /*allow_session_remember*/ true, /*allow_persistent_approval*/ true,
            ),
        },
    );

    assert_eq!(
        request,
        McpServerElicitationRequestParams {
            thread_id: session.conversation_id.to_string(),
            turn_id: Some(turn_context.sub_id),
            server_name: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
            request: McpServerElicitationRequest::Form {
                meta: Some(serde_json::json!({
                    MCP_TOOL_APPROVAL_KIND_KEY: MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL,
                    MCP_TOOL_APPROVAL_PERSIST_KEY: [
                        MCP_TOOL_APPROVAL_PERSIST_SESSION,
                        MCP_TOOL_APPROVAL_PERSIST_ALWAYS,
                    ],
                    MCP_TOOL_APPROVAL_SOURCE_KEY: MCP_TOOL_APPROVAL_SOURCE_CONNECTOR,
                    MCP_TOOL_APPROVAL_CONNECTOR_ID_KEY: "calendar",
                    MCP_TOOL_APPROVAL_CONNECTOR_NAME_KEY: "Calendar",
                    MCP_TOOL_APPROVAL_CONNECTOR_DESCRIPTION_KEY: "Manage events and schedules.",
                    MCP_TOOL_APPROVAL_TOOL_TITLE_KEY: "Create Event",
                    MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY: "Create a calendar event.",
                    MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY: {
                        "calendar_id": "primary",
                        "title": "Roadmap review",
                    },
                    MCP_TOOL_APPROVAL_TOOL_PARAMS_DISPLAY_KEY: [
                        {
                            "name": "calendar_id",
                            "value": "primary",
                            "display_name": "Calendar",
                        },
                        {
                            "name": "title",
                            "value": "Roadmap review",
                            "display_name": "Title",
                        },
                    ],
                })),
                message: "Allow Calendar to create an event?".to_string(),
                requested_schema: McpElicitationSchema {
                    schema_uri: None,
                    type_: McpElicitationObjectType::Object,
                    properties: BTreeMap::new(),
                    required: None,
                },
            },
        }
    );
}

#[test]
fn custom_mcp_tool_question_mentions_server_name() {
    let question = build_mcp_tool_approval_question(
        "q".to_string(),
        "custom_server",
        "run_action",
        /*connector_name*/ None,
        prompt_options(
            /*allow_session_remember*/ false, /*allow_persistent_approval*/ false,
        ),
        /*question_override*/ None,
    );

    assert_eq!(question.header, "Approve app tool call?");
    assert_eq!(
        question.question,
        "Allow the custom_server MCP server to run tool \"run_action\"?"
    );
    assert!(
        !question
            .options
            .expect("options")
            .into_iter()
            .map(|option| option.label)
            .any(|label| label == MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER)
    );
}

#[test]
fn praxis_apps_tool_question_uses_fallback_app_label() {
    let question = build_mcp_tool_approval_question(
        "q".to_string(),
        PRAXIS_APPS_MCP_SERVER_NAME,
        "run_action",
        /*connector_name*/ None,
        prompt_options(
            /*allow_session_remember*/ true, /*allow_persistent_approval*/ true,
        ),
        /*question_override*/ None,
    );

    assert_eq!(
        question.question,
        "Allow this app to run tool \"run_action\"?"
    );
}

#[test]
fn trusted_praxis_apps_tool_question_offers_always_allow() {
    let question = build_mcp_tool_approval_question(
        "q".to_string(),
        PRAXIS_APPS_MCP_SERVER_NAME,
        "run_action",
        Some("Calendar"),
        prompt_options(
            /*allow_session_remember*/ true, /*allow_persistent_approval*/ true,
        ),
        /*question_override*/ None,
    );
    let options = question.options.expect("options");

    assert!(options.iter().any(|option| {
        option.label == MCP_TOOL_APPROVAL_ACCEPT_FOR_SESSION
            && option.description == "Run the tool and remember this choice for this session."
    }));
    assert!(options.iter().any(|option| {
        option.label == MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER
            && option.description == "Run the tool and remember this choice for future tool calls."
    }));
    assert_eq!(
        options
            .into_iter()
            .map(|option| option.label)
            .collect::<Vec<_>>(),
        vec![
            MCP_TOOL_APPROVAL_ACCEPT.to_string(),
            MCP_TOOL_APPROVAL_ACCEPT_FOR_SESSION.to_string(),
            MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER.to_string(),
            MCP_TOOL_APPROVAL_CANCEL.to_string(),
        ]
    );
}

#[test]
fn praxis_apps_tool_question_without_elicitation_omits_always_allow() {
    let session_key = McpToolApprovalKey {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        connector_id: Some("calendar".to_string()),
        tool_name: "run_action".to_string(),
    };
    let persistent_key = session_key.clone();
    let question = build_mcp_tool_approval_question(
        "q".to_string(),
        PRAXIS_APPS_MCP_SERVER_NAME,
        "run_action",
        Some("Calendar"),
        mcp_tool_approval_prompt_options(
            Some(&session_key),
            Some(&persistent_key),
            /*tool_call_mcp_elicitation_enabled*/ false,
        ),
        /*question_override*/ None,
    );

    assert_eq!(
        question
            .options
            .expect("options")
            .into_iter()
            .map(|option| option.label)
            .collect::<Vec<_>>(),
        vec![
            MCP_TOOL_APPROVAL_ACCEPT.to_string(),
            MCP_TOOL_APPROVAL_ACCEPT_FOR_SESSION.to_string(),
            MCP_TOOL_APPROVAL_CANCEL.to_string(),
        ]
    );
}

#[test]
fn custom_mcp_tool_question_offers_session_remember_and_always_allow() {
    let question = build_mcp_tool_approval_question(
        "q".to_string(),
        "custom_server",
        "run_action",
        /*connector_name*/ None,
        prompt_options(
            /*allow_session_remember*/ true, /*allow_persistent_approval*/ true,
        ),
        /*question_override*/ None,
    );

    assert_eq!(
        question
            .options
            .expect("options")
            .into_iter()
            .map(|option| option.label)
            .collect::<Vec<_>>(),
        vec![
            MCP_TOOL_APPROVAL_ACCEPT.to_string(),
            MCP_TOOL_APPROVAL_ACCEPT_FOR_SESSION.to_string(),
            MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER.to_string(),
            MCP_TOOL_APPROVAL_CANCEL.to_string(),
        ]
    );
}

#[test]
fn custom_servers_support_session_and_persistent_approval() {
    let invocation = McpInvocation {
        server: "custom_server".to_string(),
        tool: "run_action".to_string(),
        arguments: None,
    };
    let expected = McpToolApprovalKey {
        server: "custom_server".to_string(),
        connector_id: None,
        tool_name: "run_action".to_string(),
    };

    assert_eq!(
        session_mcp_tool_approval_key(&invocation, /*metadata*/ None, AppToolApproval::Auto),
        Some(expected.clone())
    );
    assert_eq!(
        persistent_mcp_tool_approval_key(
            &invocation,
            /*metadata*/ None,
            AppToolApproval::Auto
        ),
        Some(expected)
    );
}

#[test]
fn praxis_apps_connectors_support_persistent_approval() {
    let invocation = McpInvocation {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        tool: "calendar/list_events".to_string(),
        arguments: None,
    };
    let metadata = approval_metadata(
        Some("calendar"),
        Some("Calendar"),
        /*connector_description*/ None,
        /*tool_title*/ None,
        /*tool_description*/ None,
    );
    let expected = McpToolApprovalKey {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        connector_id: Some("calendar".to_string()),
        tool_name: "calendar/list_events".to_string(),
    };

    assert_eq!(
        session_mcp_tool_approval_key(&invocation, Some(&metadata), AppToolApproval::Auto),
        Some(expected.clone())
    );
    assert_eq!(
        persistent_mcp_tool_approval_key(&invocation, Some(&metadata), AppToolApproval::Auto),
        Some(expected)
    );
}

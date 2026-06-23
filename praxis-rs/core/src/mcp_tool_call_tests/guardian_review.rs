use super::*;

#[test]
fn guardian_mcp_review_request_includes_invocation_metadata() {
    let invocation = McpInvocation {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        tool: "browser_navigate".to_string(),
        arguments: Some(serde_json::json!({
            "url": "https://example.com",
        })),
    };

    let request = build_guardian_mcp_tool_review_request(
        "call-1",
        &invocation,
        Some(&approval_metadata(
            Some("playwright"),
            Some("Playwright"),
            Some("Browser automation"),
            Some("Navigate"),
            Some("Open a page"),
        )),
    );

    assert_eq!(
        request,
        GuardianApprovalRequest::McpToolCall {
            id: "call-1".to_string(),
            server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
            tool_name: "browser_navigate".to_string(),
            arguments: Some(serde_json::json!({
                "url": "https://example.com",
            })),
            connector_id: Some("playwright".to_string()),
            connector_name: Some("Playwright".to_string()),
            connector_description: Some("Browser automation".to_string()),
            tool_title: Some("Navigate".to_string()),
            tool_description: Some("Open a page".to_string()),
            annotations: None,
        }
    );
}

#[test]
fn guardian_mcp_review_request_includes_annotations_when_present() {
    let invocation = McpInvocation {
        server: "custom_server".to_string(),
        tool: "dangerous_tool".to_string(),
        arguments: None,
    };
    let metadata = McpToolApprovalMetadata {
        annotations: Some(annotations(Some(false), Some(true), Some(true))),
        connector_id: None,
        connector_name: None,
        connector_description: None,
        tool_title: None,
        tool_description: None,
        praxis_apps_meta: None,
    };

    let request = build_guardian_mcp_tool_review_request("call-1", &invocation, Some(&metadata));

    assert_eq!(
        request,
        GuardianApprovalRequest::McpToolCall {
            id: "call-1".to_string(),
            server: "custom_server".to_string(),
            tool_name: "dangerous_tool".to_string(),
            arguments: None,
            connector_id: None,
            connector_name: None,
            connector_description: None,
            tool_title: None,
            tool_description: None,
            annotations: Some(GuardianMcpAnnotations {
                destructive_hint: Some(true),
                open_world_hint: Some(true),
                read_only_hint: Some(false),
            }),
        }
    );
}

#[test]
fn prepare_arc_request_action_serializes_mcp_tool_call_shape() {
    let invocation = McpInvocation {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        tool: "browser_navigate".to_string(),
        arguments: Some(serde_json::json!({
            "url": "https://example.com",
        })),
    };

    let action = prepare_arc_request_action(
        &invocation,
        Some(&approval_metadata(
            /*connector_id*/ None,
            Some("Playwright"),
            /*connector_description*/ None,
            Some("Navigate"),
            /*tool_description*/ None,
        )),
    );

    assert_eq!(
        action,
        serde_json::json!({
            "tool": "mcp_tool_call",
            "server": PRAXIS_APPS_MCP_SERVER_NAME,
            "tool_name": "browser_navigate",
            "arguments": {
                "url": "https://example.com",
            },
            "connector_name": "Playwright",
            "tool_title": "Navigate",
        })
    );
}

#[test]
fn guardian_review_decision_maps_to_mcp_tool_decision() {
    assert_eq!(
        mcp_tool_approval_decision_from_guardian(ReviewDecision::Approved),
        McpToolApprovalDecision::Accept
    );
    assert_eq!(
        mcp_tool_approval_decision_from_guardian(ReviewDecision::Denied),
        McpToolApprovalDecision::Decline
    );
    assert_eq!(
        mcp_tool_approval_decision_from_guardian(ReviewDecision::Abort),
        McpToolApprovalDecision::Decline
    );
}

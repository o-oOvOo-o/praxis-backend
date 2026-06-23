use super::*;

#[tokio::test]
async fn mcp_tool_call_request_meta_includes_turn_metadata_for_custom_server() {
    let (_, turn_context) = make_session_and_context().await;
    let expected_turn_metadata = serde_json::from_str::<serde_json::Value>(
        &turn_context
            .turn_metadata_state
            .current_header_value()
            .expect("turn metadata header"),
    )
    .expect("turn metadata json");

    let meta =
        build_mcp_tool_call_request_meta(&turn_context, "custom_server", /*metadata*/ None)
            .expect("custom servers should receive turn metadata");

    assert_eq!(
        meta,
        serde_json::json!({
            crate::X_PRAXIS_TURN_METADATA_HEADER: expected_turn_metadata,
        })
    );
}

#[tokio::test]
async fn praxis_apps_tool_call_request_meta_includes_turn_metadata_and_praxis_apps_meta() {
    let (_, turn_context) = make_session_and_context().await;
    let expected_turn_metadata = serde_json::from_str::<serde_json::Value>(
        &turn_context
            .turn_metadata_state
            .current_header_value()
            .expect("turn metadata header"),
    )
    .expect("turn metadata json");
    let metadata = McpToolApprovalMetadata {
        annotations: None,
        connector_id: Some("calendar".to_string()),
        connector_name: Some("Calendar".to_string()),
        connector_description: Some("Manage events".to_string()),
        tool_title: Some("Create Event".to_string()),
        tool_description: Some("Create a calendar event.".to_string()),
        praxis_apps_meta: Some(
            serde_json::json!({
                "resource_uri": "connector://calendar/tools/calendar_create_event",
                "contains_mcp_source": true,
                "connector_id": "calendar",
            })
            .as_object()
            .cloned()
            .expect("_praxis_apps metadata should be an object"),
        ),
    };

    assert_eq!(
        build_mcp_tool_call_request_meta(
            &turn_context,
            PRAXIS_APPS_MCP_SERVER_NAME,
            Some(&metadata),
        ),
        Some(serde_json::json!({
            crate::X_PRAXIS_TURN_METADATA_HEADER: expected_turn_metadata,
            MCP_TOOL_PRAXIS_APPS_META_KEY: {
                "resource_uri": "connector://calendar/tools/calendar_create_event",
                "contains_mcp_source": true,
                "connector_id": "calendar",
            },
        }))
    );
}

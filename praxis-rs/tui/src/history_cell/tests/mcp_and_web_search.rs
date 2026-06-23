use super::*;

#[test]
fn web_search_history_cell_snapshot() {
    let query = "example search query with several generic words to exercise wrapping".to_string();
    let cell = new_web_search_call(
        "call-1".to_string(),
        query.clone(),
        WebSearchAction::Search {
            query: Some(query),
            queries: None,
        },
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 64)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn web_search_history_cell_wraps_with_indented_continuation() {
    let query = "example search query with several generic words to exercise wrapping".to_string();
    let cell = new_web_search_call(
        "call-1".to_string(),
        query.clone(),
        WebSearchAction::Search {
            query: Some(query),
            queries: None,
        },
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 64));

    assert_eq!(
        rendered,
        vec![
            "• Searched example search query with several generic words to".to_string(),
            "  exercise wrapping".to_string(),
        ]
    );
}

#[test]
fn web_search_history_cell_short_query_does_not_wrap() {
    let query = "short query".to_string();
    let cell = new_web_search_call(
        "call-1".to_string(),
        query.clone(),
        WebSearchAction::Search {
            query: Some(query),
            queries: None,
        },
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 64));

    assert_eq!(rendered, vec!["• Searched short query".to_string()]);
}

#[test]
fn web_search_history_cell_transcript_snapshot() {
    let query = "example search query with several generic words to exercise wrapping".to_string();
    let cell = new_web_search_call(
        "call-1".to_string(),
        query.clone(),
        WebSearchAction::Search {
            query: Some(query),
            queries: None,
        },
    );
    let rendered = render_lines(&cell.transcript_lines(/*width*/ 64)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn active_mcp_tool_call_snapshot() {
    let invocation = McpInvocation {
        server: "search".into(),
        tool: "find_docs".into(),
        arguments: Some(json!({
            "query": "ratatui styling",
            "limit": 3,
        })),
    };

    let cell = new_active_mcp_tool_call(
        "call-1".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn mcp_inventory_loading_snapshot() {
    let cell = new_mcp_inventory_loading(/*animations_enabled*/ true);
    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn completed_mcp_tool_call_success_snapshot() {
    let invocation = McpInvocation {
        server: "search".into(),
        tool: "find_docs".into(),
        arguments: Some(json!({
            "query": "ratatui styling",
            "limit": 3,
        })),
    };

    let result = CallToolResult {
        content: vec![text_block("Found styling guidance in styles.md")],
        is_error: None,
        structured_content: None,
        meta: None,
    };

    let mut cell = new_active_mcp_tool_call(
        "call-2".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    assert!(
        cell.complete(Duration::from_millis(1420), Ok(result))
            .is_none()
    );

    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn completed_mcp_tool_call_image_after_text_returns_extra_cell() {
    let invocation = McpInvocation {
        server: "image".into(),
        tool: "generate".into(),
        arguments: Some(json!({
            "prompt": "tiny image",
        })),
    };

    let result = CallToolResult {
        content: vec![
            text_block("Here is the image:"),
            image_block(SMALL_PNG_BASE64),
        ],
        is_error: None,
        structured_content: None,
        meta: None,
    };

    let mut cell = new_active_mcp_tool_call(
        "call-image".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    let extra_cell = cell
        .complete(Duration::from_millis(25), Ok(result))
        .expect("expected image cell");

    let rendered = render_lines(&extra_cell.display_lines(/*width*/ 80));
    assert_eq!(rendered, vec!["tool result (image output)"]);
}

#[test]
fn completed_mcp_tool_call_accepts_data_url_image_blocks() {
    let invocation = McpInvocation {
        server: "image".into(),
        tool: "generate".into(),
        arguments: Some(json!({
            "prompt": "tiny image",
        })),
    };

    let data_url = format!("data:image/png;base64,{SMALL_PNG_BASE64}");
    let result = CallToolResult {
        content: vec![image_block(&data_url)],
        is_error: None,
        structured_content: None,
        meta: None,
    };

    let mut cell = new_active_mcp_tool_call(
        "call-image-data-url".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    let extra_cell = cell
        .complete(Duration::from_millis(25), Ok(result))
        .expect("expected image cell");

    let rendered = render_lines(&extra_cell.display_lines(/*width*/ 80));
    assert_eq!(rendered, vec!["tool result (image output)"]);
}

#[test]
fn completed_mcp_tool_call_skips_invalid_image_blocks() {
    let invocation = McpInvocation {
        server: "image".into(),
        tool: "generate".into(),
        arguments: Some(json!({
            "prompt": "tiny image",
        })),
    };

    let result = CallToolResult {
        content: vec![image_block("not-base64"), image_block(SMALL_PNG_BASE64)],
        is_error: None,
        structured_content: None,
        meta: None,
    };

    let mut cell = new_active_mcp_tool_call(
        "call-image-2".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    let extra_cell = cell
        .complete(Duration::from_millis(25), Ok(result))
        .expect("expected image cell");

    let rendered = render_lines(&extra_cell.display_lines(/*width*/ 80));
    assert_eq!(rendered, vec!["tool result (image output)"]);
}

#[test]
fn completed_mcp_tool_call_error_snapshot() {
    let invocation = McpInvocation {
        server: "search".into(),
        tool: "find_docs".into(),
        arguments: Some(json!({
            "query": "ratatui styling",
            "limit": 3,
        })),
    };

    let mut cell = new_active_mcp_tool_call(
        "call-3".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    assert!(
        cell.complete(Duration::from_secs(2), Err("network timeout".into()))
            .is_none()
    );

    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn completed_mcp_tool_call_multiple_outputs_snapshot() {
    let invocation = McpInvocation {
        server: "search".into(),
        tool: "find_docs".into(),
        arguments: Some(json!({
            "query": "ratatui styling",
            "limit": 3,
        })),
    };

    let result = CallToolResult {
        content: vec![
            text_block(
                "Found styling guidance in styles.md and additional notes in CONTRIBUTING.md.",
            ),
            resource_link_block(
                "file:///docs/styles.md",
                "styles.md",
                Some("Styles"),
                Some("Link to styles documentation"),
            ),
        ],
        is_error: None,
        structured_content: None,
        meta: None,
    };

    let mut cell = new_active_mcp_tool_call(
        "call-4".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    assert!(
        cell.complete(Duration::from_millis(640), Ok(result))
            .is_none()
    );

    let rendered = render_lines(&cell.display_lines(/*width*/ 48)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn completed_mcp_tool_call_wrapped_outputs_snapshot() {
    let invocation = McpInvocation {
        server: "metrics".into(),
        tool: "get_nearby_metric".into(),
        arguments: Some(json!({
            "query": "very_long_query_that_needs_wrapping_to_display_properly_in_the_history",
            "limit": 1,
        })),
    };

    let result = CallToolResult {
        content: vec![text_block(
            "Line one of the response, which is quite long and needs wrapping.\nLine two continues the response with more detail.",
        )],
        is_error: None,
        structured_content: None,
        meta: None,
    };

    let mut cell = new_active_mcp_tool_call(
        "call-5".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    assert!(
        cell.complete(Duration::from_millis(1280), Ok(result))
            .is_none()
    );

    let rendered = render_lines(&cell.display_lines(/*width*/ 40)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn completed_mcp_tool_call_multiple_outputs_inline_snapshot() {
    let invocation = McpInvocation {
        server: "metrics".into(),
        tool: "summary".into(),
        arguments: Some(json!({
            "metric": "trace.latency",
            "window": "15m",
        })),
    };

    let result = CallToolResult {
        content: vec![
            text_block("Latency summary: p50=120ms, p95=480ms."),
            text_block("No anomalies detected."),
        ],
        is_error: None,
        structured_content: None,
        meta: None,
    };

    let mut cell = new_active_mcp_tool_call(
        "call-6".into(),
        invocation,
        /*animations_enabled*/ true,
    );
    assert!(
        cell.complete(Duration::from_millis(320), Ok(result))
            .is_none()
    );

    let rendered = render_lines(&cell.display_lines(/*width*/ 120)).join("\n");

    insta::assert_snapshot!(rendered);
}

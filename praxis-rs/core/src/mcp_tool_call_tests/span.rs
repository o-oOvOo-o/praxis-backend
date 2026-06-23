use super::*;

#[tokio::test]
async fn mcp_tool_call_span_records_expected_fields() {
    let buffer: &'static std::sync::Mutex<Vec<u8>> =
        Box::leak(Box::new(std::sync::Mutex::new(Vec::new())));
    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .with_ansi(false)
        .with_max_level(Level::TRACE)
        .with_span_events(FmtSpan::FULL)
        .with_writer(MockWriter::new(buffer))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let (session, turn_context) = make_session_and_context().await;

    async {}
        .instrument(mcp_tool_call_span(
            &session,
            &turn_context,
            McpToolCallSpanFields {
                server_name: "rmcp",
                tool_name: "echo",
                call_id: "call-123",
                server_origin: Some("https://example.com:8443/mcp"),
                connector_id: Some("calendar"),
                connector_name: Some("Calendar"),
            },
        ))
        .await;

    let logs = String::from_utf8(buffer.lock().expect("buffer lock").clone()).expect("utf8 logs");
    assert!(
        logs.contains("mcp.tools.call{otel.kind=\"client\"")
            && logs.contains("rpc.system=\"jsonrpc\"")
            && logs.contains("rpc.method=\"tools/call\"")
            && logs.contains("mcp.server.name=\"rmcp\"")
            && logs.contains("mcp.server.origin=\"https://example.com:8443/mcp\"")
            && logs.contains("mcp.transport=\"streamable_http\"")
            && logs.contains("mcp.connector.id=\"calendar\"")
            && logs.contains("mcp.connector.name=\"Calendar\"")
            && logs.contains("tool.name=\"echo\"")
            && logs.contains("tool.call_id=\"call-123\"")
            && logs.contains("server.address=\"example.com\"")
            && logs.contains("server.port=8443")
            && logs.contains("conversation.id=")
            && logs.contains("session.id=")
            && logs.contains("turn.id="),
        "missing MCP tool span fields\nlogs:\n{logs}"
    );
}

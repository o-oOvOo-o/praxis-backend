use praxis_app_gateway_protocol::McpToolCallError;
use praxis_app_gateway_protocol::McpToolCallResult;
use praxis_app_gateway_protocol::McpToolCallStatus;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_protocol::protocol::McpToolCallBeginEvent;
use praxis_protocol::protocol::McpToolCallEndEvent;

type JsonValue = serde_json::Value;

pub(crate) fn construct_mcp_tool_call_item(begin_event: McpToolCallBeginEvent) -> ThreadItem {
    ThreadItem::McpToolCall {
        id: begin_event.call_id,
        server: begin_event.invocation.server,
        tool: begin_event.invocation.tool,
        status: McpToolCallStatus::InProgress,
        arguments: begin_event.invocation.arguments.unwrap_or(JsonValue::Null),
        result: None,
        error: None,
        duration_ms: None,
    }
}

pub(crate) fn construct_mcp_tool_call_end_item(end_event: McpToolCallEndEvent) -> ThreadItem {
    let status = if end_event.is_success() {
        McpToolCallStatus::Completed
    } else {
        McpToolCallStatus::Failed
    };
    let duration_ms = i64::try_from(end_event.duration.as_millis()).ok();

    let (result, error) = match &end_event.result {
        Ok(value) => (
            Some(McpToolCallResult {
                content: value.content.clone(),
                structured_content: value.structured_content.clone(),
            }),
            None,
        ),
        Err(message) => (
            None,
            Some(McpToolCallError {
                message: message.clone(),
            }),
        ),
    };

    ThreadItem::McpToolCall {
        id: end_event.call_id,
        server: end_event.invocation.server,
        tool: end_event.invocation.tool,
        status,
        arguments: end_event.invocation.arguments.unwrap_or(JsonValue::Null),
        result,
        error,
        duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_protocol::mcp::CallToolResult;
    use praxis_protocol::protocol::McpInvocation;
    use pretty_assertions::assert_eq;
    use rmcp::model::Content;
    use std::time::Duration;

    #[test]
    fn construct_mcp_tool_call_begin_item_with_args() {
        let begin_event = McpToolCallBeginEvent {
            call_id: "call_123".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: Some(serde_json::json!({"server": ""})),
            },
        };

        let item = construct_mcp_tool_call_item(begin_event.clone());

        let expected = ThreadItem::McpToolCall {
            id: begin_event.call_id,
            server: begin_event.invocation.server,
            tool: begin_event.invocation.tool,
            status: McpToolCallStatus::InProgress,
            arguments: serde_json::json!({"server": ""}),
            result: None,
            error: None,
            duration_ms: None,
        };

        assert_eq!(item, expected);
    }

    #[test]
    fn construct_mcp_tool_call_begin_item_without_args() {
        let begin_event = McpToolCallBeginEvent {
            call_id: "call_456".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: None,
            },
        };

        let item = construct_mcp_tool_call_item(begin_event.clone());

        let expected = ThreadItem::McpToolCall {
            id: begin_event.call_id,
            server: begin_event.invocation.server,
            tool: begin_event.invocation.tool,
            status: McpToolCallStatus::InProgress,
            arguments: JsonValue::Null,
            result: None,
            error: None,
            duration_ms: None,
        };

        assert_eq!(item, expected);
    }

    #[test]
    fn construct_mcp_tool_call_end_item_success() {
        let content = vec![
            serde_json::to_value(Content::text("{\"resources\":[]}"))
                .expect("content should serialize"),
        ];
        let result = CallToolResult {
            content: content.clone(),
            is_error: Some(false),
            structured_content: None,
            meta: None,
        };

        let end_event = McpToolCallEndEvent {
            call_id: "call_789".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: Some(serde_json::json!({"server": ""})),
            },
            duration: Duration::from_nanos(92708),
            result: Ok(result),
        };

        let item = construct_mcp_tool_call_end_item(end_event.clone());

        let expected = ThreadItem::McpToolCall {
            id: end_event.call_id,
            server: end_event.invocation.server,
            tool: end_event.invocation.tool,
            status: McpToolCallStatus::Completed,
            arguments: serde_json::json!({"server": ""}),
            result: Some(McpToolCallResult {
                content,
                structured_content: None,
            }),
            error: None,
            duration_ms: Some(0),
        };

        assert_eq!(item, expected);
    }

    #[test]
    fn construct_mcp_tool_call_end_item_error() {
        let end_event = McpToolCallEndEvent {
            call_id: "call_err".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: None,
            },
            duration: Duration::from_millis(1),
            result: Err("boom".to_string()),
        };

        let item = construct_mcp_tool_call_end_item(end_event.clone());

        let expected = ThreadItem::McpToolCall {
            id: end_event.call_id,
            server: end_event.invocation.server,
            tool: end_event.invocation.tool,
            status: McpToolCallStatus::Failed,
            arguments: JsonValue::Null,
            result: None,
            error: Some(McpToolCallError {
                message: "boom".to_string(),
            }),
            duration_ms: Some(1),
        };

        assert_eq!(item, expected);
    }
}

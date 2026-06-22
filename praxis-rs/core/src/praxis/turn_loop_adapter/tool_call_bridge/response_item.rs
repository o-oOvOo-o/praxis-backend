use praxis_loop::tool::ToolCall as LoopToolCall;
use praxis_protocol::models::LocalShellStatus;
use praxis_protocol::models::ResponseItem;
use serde_json::json;

use super::super::local_shell_bridge;
use super::metadata;
use super::metadata::OriginalResponseItemProjection;
use super::metadata::PayloadKind;

enum ToolSearchArgumentsProjection {
    Parsed(serde_json::Value),
    QueryFallback(serde_json::Value),
}

impl ToolSearchArgumentsProjection {
    fn into_value(self) -> serde_json::Value {
        match self {
            Self::Parsed(value) | Self::QueryFallback(value) => value,
        }
    }
}

pub(super) fn loop_tool_call_to_response_item(call: &LoopToolCall) -> ResponseItem {
    match metadata::original_response_item_projection(call) {
        OriginalResponseItemProjection::Restored(item) => return item,
        OriginalResponseItemProjection::Reconstruct => {}
    }

    match metadata::payload_kind(call) {
        PayloadKind::ToolSearch => ResponseItem::ToolSearchCall {
            id: None,
            call_id: Some(call.id.clone()),
            status: None,
            execution: "client".to_string(),
            arguments: tool_search_arguments_projection(call.arguments.as_str()).into_value(),
        },
        PayloadKind::Custom => ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: call.id.clone(),
            name: call.name.clone(),
            input: call.arguments.clone(),
        },
        PayloadKind::LocalShell => ResponseItem::LocalShellCall {
            id: None,
            call_id: Some(call.id.clone()),
            status: LocalShellStatus::InProgress,
            action: local_shell_bridge::exec_action_from_arguments(&call.arguments),
        },
        PayloadKind::Function | PayloadKind::Mcp => ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: call.name.clone(),
            namespace: call.namespace.clone(),
            arguments: call.arguments.clone(),
            call_id: call.id.clone(),
        },
    }
}

fn tool_search_arguments_projection(arguments: &str) -> ToolSearchArgumentsProjection {
    serde_json::from_str(arguments).map_or_else(
        |_| ToolSearchArgumentsProjection::QueryFallback(json!({ "query": arguments })),
        ToolSearchArgumentsProjection::Parsed,
    )
}

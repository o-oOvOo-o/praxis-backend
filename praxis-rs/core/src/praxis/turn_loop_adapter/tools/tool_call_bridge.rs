use praxis_loop::tool::ToolCall as LoopToolCall;
use praxis_protocol::models::ResponseItem;

use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::tools::router::ToolCall as CoreToolCall;
use crate::tools::router::ToolRouter;

mod metadata {
    mod json_args {
        use praxis_loop::outcome::TurnError;
        use praxis_loop::outcome::TurnErrorKind;
        use praxis_loop::tool::ToolCall as LoopToolCall;

        use super::PayloadKind;

        pub(in crate::praxis::turn_loop_adapter) fn parse_arguments<T>(
            call: &LoopToolCall,
            kind: PayloadKind,
        ) -> Result<T, TurnError>
        where
            T: serde::de::DeserializeOwned,
        {
            serde_json::from_str(&call.arguments).map_err(|err| {
                TurnError::new(
                    TurnErrorKind::Tool,
                    format!("failed to parse {} tool arguments: {err}", kind.as_str()),
                )
            })
        }
    }
    mod kind {
        use praxis_loop::tool::ToolCall as LoopToolCall;

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(in crate::praxis::turn_loop_adapter) enum PayloadKind {
            Function,
            Mcp,
            ToolSearch,
            Custom,
            LocalShell,
        }

        const META_PAYLOAD_KIND: &str = "praxis.payload.kind";

        impl PayloadKind {
            pub(in crate::praxis::turn_loop_adapter) fn as_str(self) -> &'static str {
                match self {
                    Self::Function => "function",
                    Self::Mcp => "mcp",
                    Self::ToolSearch => "tool_search",
                    Self::Custom => "custom",
                    Self::LocalShell => "local_shell",
                }
            }

            fn from_metadata(value: Option<&str>) -> Self {
                match value {
                    Some("mcp") => Self::Mcp,
                    Some("tool_search") => Self::ToolSearch,
                    Some("custom") => Self::Custom,
                    Some("local_shell") => Self::LocalShell,
                    Some("function") | None | Some(_) => Self::Function,
                }
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn insert_payload_kind(
            metadata: &mut std::collections::BTreeMap<String, String>,
            kind: PayloadKind,
        ) {
            metadata.insert(META_PAYLOAD_KIND.to_string(), kind.as_str().to_string());
        }

        pub(in crate::praxis::turn_loop_adapter) fn payload_kind(
            call: &LoopToolCall,
        ) -> PayloadKind {
            PayloadKind::from_metadata(call.metadata.get(META_PAYLOAD_KIND).map(String::as_str))
        }
    }
    mod mcp {
        use std::collections::BTreeMap;

        use praxis_loop::outcome::TurnError;
        use praxis_loop::outcome::TurnErrorKind;
        use praxis_loop::tool::ToolCall as LoopToolCall;

        const META_MCP_SERVER: &str = "praxis.mcp.server";
        const META_MCP_TOOL: &str = "praxis.mcp.tool";

        pub(in crate::praxis::turn_loop_adapter) fn insert_mcp(
            metadata: &mut BTreeMap<String, String>,
            server: String,
            tool: String,
        ) {
            metadata.insert(META_MCP_SERVER.to_string(), server);
            metadata.insert(META_MCP_TOOL.to_string(), tool);
        }

        pub(in crate::praxis::turn_loop_adapter) fn mcp_server(
            call: &LoopToolCall,
        ) -> Result<String, TurnError> {
            metadata_value(call, META_MCP_SERVER)
        }

        pub(in crate::praxis::turn_loop_adapter) fn mcp_tool(
            call: &LoopToolCall,
        ) -> Result<String, TurnError> {
            metadata_value(call, META_MCP_TOOL)
        }

        fn metadata_value(call: &LoopToolCall, key: &str) -> Result<String, TurnError> {
            call.metadata.get(key).cloned().ok_or_else(|| {
                TurnError::new(
                    TurnErrorKind::Tool,
                    format!("tool call `{}` is missing metadata `{key}`", call.name),
                )
            })
        }
    }
    mod original_item {
        use std::collections::BTreeMap;

        use praxis_loop::tool::ToolCall as LoopToolCall;
        use praxis_protocol::models::ResponseItem;

        pub(in crate::praxis::turn_loop_adapter) enum OriginalResponseItemProjection {
            Restored(ResponseItem),
            Reconstruct,
        }

        const META_ORIGINAL_RESPONSE_ITEM: &str = "praxis.response_item";

        pub(in crate::praxis::turn_loop_adapter) fn from_source_item(
            source_item: Option<&ResponseItem>,
        ) -> BTreeMap<String, String> {
            let mut metadata = BTreeMap::new();
            if let Some(item) = source_item
                && let Ok(serialized) = serde_json::to_string(item)
            {
                metadata.insert(META_ORIGINAL_RESPONSE_ITEM.to_string(), serialized);
            }
            metadata
        }

        pub(in crate::praxis::turn_loop_adapter) fn original_response_item_projection(
            call: &LoopToolCall,
        ) -> OriginalResponseItemProjection {
            let Some(value) = call.metadata.get(META_ORIGINAL_RESPONSE_ITEM) else {
                return OriginalResponseItemProjection::Reconstruct;
            };
            serde_json::from_str(value).map_or(
                OriginalResponseItemProjection::Reconstruct,
                OriginalResponseItemProjection::Restored,
            )
        }
    }

    pub(in crate::praxis::turn_loop_adapter) use json_args::parse_arguments;
    pub(in crate::praxis::turn_loop_adapter) use kind::PayloadKind;
    pub(in crate::praxis::turn_loop_adapter) use kind::insert_payload_kind;
    pub(in crate::praxis::turn_loop_adapter) use kind::payload_kind;
    pub(in crate::praxis::turn_loop_adapter) use mcp::insert_mcp;
    pub(in crate::praxis::turn_loop_adapter) use mcp::mcp_server;
    pub(in crate::praxis::turn_loop_adapter) use mcp::mcp_tool;
    pub(in crate::praxis::turn_loop_adapter) use original_item::OriginalResponseItemProjection;
    pub(in crate::praxis::turn_loop_adapter) use original_item::from_source_item;
    pub(in crate::praxis::turn_loop_adapter) use original_item::original_response_item_projection;
}
mod payload_decoder {
    use praxis_loop::outcome::TurnError;
    use praxis_loop::tool::ToolCall as LoopToolCall;

    use crate::tools::context::ToolPayload;

    use super::metadata;
    use super::metadata::PayloadKind;

    pub(in crate::praxis::turn_loop_adapter) fn decode_payload(
        call: &LoopToolCall,
    ) -> Result<ToolPayload, TurnError> {
        match metadata::payload_kind(call) {
            PayloadKind::Mcp => Ok(ToolPayload::Mcp {
                server: metadata::mcp_server(call)?,
                tool: metadata::mcp_tool(call)?,
                raw_arguments: call.arguments.clone(),
            }),
            PayloadKind::ToolSearch => Ok(ToolPayload::ToolSearch {
                arguments: metadata::parse_arguments(call, PayloadKind::ToolSearch)?,
            }),
            PayloadKind::Custom => Ok(ToolPayload::Custom {
                input: call.arguments.clone(),
            }),
            PayloadKind::LocalShell => Ok(ToolPayload::LocalShell {
                params: metadata::parse_arguments(call, PayloadKind::LocalShell)?,
            }),
            PayloadKind::Function => Ok(ToolPayload::Function {
                arguments: call.arguments.clone(),
            }),
        }
    }
}
mod payload_encoder {
    use std::collections::BTreeMap;

    use serde_json::json;

    use crate::tools::context::ToolPayload;

    use super::super::local_shell_bridge;
    use super::metadata;
    use super::metadata::PayloadKind;

    enum EncodedToolSearchArguments {
        Serialized(String),
        SerializationError(String),
    }

    impl EncodedToolSearchArguments {
        fn into_arguments(self) -> String {
            match self {
                Self::Serialized(arguments) | Self::SerializationError(arguments) => arguments,
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn encode_payload(
        payload: ToolPayload,
        metadata: &mut BTreeMap<String, String>,
    ) -> String {
        let (arguments, kind) = match payload {
            ToolPayload::Function { arguments } => (arguments, PayloadKind::Function),
            ToolPayload::Mcp {
                server,
                tool,
                raw_arguments,
            } => {
                metadata::insert_mcp(metadata, server, tool);
                (raw_arguments, PayloadKind::Mcp)
            }
            ToolPayload::ToolSearch { arguments } => (
                encode_tool_search_arguments(&arguments).into_arguments(),
                PayloadKind::ToolSearch,
            ),
            ToolPayload::Custom { input } => (input, PayloadKind::Custom),
            ToolPayload::LocalShell { params } => (
                local_shell_bridge::params_to_json(&params),
                PayloadKind::LocalShell,
            ),
        };

        metadata::insert_payload_kind(metadata, kind);
        arguments
    }

    fn encode_tool_search_arguments<T: serde::Serialize>(
        arguments: &T,
    ) -> EncodedToolSearchArguments {
        serde_json::to_string(arguments).map_or_else(
            |err| {
                EncodedToolSearchArguments::SerializationError(
                    json!({ "serialization_error": err.to_string() }).to_string(),
                )
            },
            EncodedToolSearchArguments::Serialized,
        )
    }
}
mod response_item {
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

    pub(in crate::praxis::turn_loop_adapter) fn loop_tool_call_to_response_item(
        call: &LoopToolCall,
    ) -> ResponseItem {
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
}

pub(in crate::praxis::turn_loop_adapter) enum ResponseItemToolCall {
    ToolCall(LoopToolCall),
    NotToolCall,
}

pub(in crate::praxis::turn_loop_adapter) async fn response_item_to_loop_tool_call(
    session: &Session,
    item: ResponseItem,
) -> Result<ResponseItemToolCall, FunctionCallError> {
    let Some(call) = ToolRouter::build_tool_call(session, item.clone()).await? else {
        return Ok(ResponseItemToolCall::NotToolCall);
    };
    Ok(ResponseItemToolCall::ToolCall(
        core_tool_call_to_loop_tool_call(call, Some(&item)),
    ))
}

pub(in crate::praxis::turn_loop_adapter) fn core_tool_call_to_loop_tool_call(
    call: CoreToolCall,
    source_item: Option<&ResponseItem>,
) -> LoopToolCall {
    let CoreToolCall {
        tool_name,
        tool_namespace,
        call_id,
        payload,
    } = call;

    let mut metadata = metadata::from_source_item(source_item);
    let arguments = payload_encoder::encode_payload(payload, &mut metadata);

    LoopToolCall {
        id: call_id,
        name: tool_name,
        namespace: tool_namespace,
        arguments,
        metadata,
    }
}

pub(in crate::praxis::turn_loop_adapter) fn loop_tool_call_to_core_tool_call(
    call: LoopToolCall,
) -> Result<CoreToolCall, praxis_loop::outcome::TurnError> {
    let payload = payload_decoder::decode_payload(&call)?;
    Ok(CoreToolCall {
        tool_name: call.name,
        tool_namespace: call.namespace,
        call_id: call.id,
        payload,
    })
}

pub(in crate::praxis::turn_loop_adapter) fn loop_tool_call_to_response_item(
    call: &LoopToolCall,
) -> ResponseItem {
    response_item::loop_tool_call_to_response_item(call)
}

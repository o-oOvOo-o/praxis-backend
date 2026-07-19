//! Tool execution bridges, loop services, and canonical event emission.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter) mod tool_bridge {
    use async_trait::async_trait;
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::outcome::TurnError;
    use praxis_loop::outcome::TurnErrorKind;
    use praxis_loop::tool::PreparedToolCall;
    use praxis_loop::tool::Tool;
    use praxis_loop::tool::ToolCall as LoopToolCall;
    use praxis_loop::tool::ToolExecutionContext;
    use praxis_loop::tool::ToolLifecycleSink;
    use praxis_loop::tool::ToolResult as LoopToolResult;
    use praxis_loop::tool::ToolSpec as LoopToolSpec;
    use std::sync::Arc;

    use crate::error::PraxisErr;
    use crate::tools::registry::ToolPreparation as HandlerPreparation;
    use crate::tools::tool_call_runtime::ToolCallRuntime;

    use super::tool_call_bridge::loop_tool_call_to_core_tool_call;
    use super::tool_result_bridge::core_tool_description;
    use super::tool_result_bridge::response_input_to_loop_tool_result;

    pub(in crate::praxis::turn_loop_adapter) fn resolve_tool_from_runtime(
        runtime: &ToolCallRuntime,
        name: &str,
    ) -> Option<Arc<dyn Tool>> {
        let spec = runtime.find_spec(name)?;
        Some(Arc::new(PraxisLoopTool {
            runtime: runtime.clone(),
            spec: loop_tool_spec(name, &spec),
        }))
    }

    struct PraxisLoopTool {
        runtime: ToolCallRuntime,
        spec: LoopToolSpec,
    }

    #[async_trait]
    impl Tool for PraxisLoopTool {
        fn spec(&self) -> LoopToolSpec {
            self.spec.clone()
        }

        async fn prepare(&self, call: &LoopToolCall) -> LoopResult<PreparedToolCall> {
            let core_call = loop_tool_call_to_core_tool_call(call.clone())?;
            let preparation = self
                .runtime
                .prepare_tool_call(core_call)
                .await
                .map_err(|err| TurnError::new(TurnErrorKind::Tool, err.to_string()))?;
            Ok(PreparedToolCall::new(preparation.effects().clone()).with_state(preparation))
        }

        async fn execute(
            &self,
            call: LoopToolCall,
            context: ToolExecutionContext,
        ) -> LoopResult<LoopToolResult> {
            let core_call = loop_tool_call_to_core_tool_call(call)?;

            let response = self
                .runtime
                .clone()
                .handle_tool_call_observed(core_call, context.cancel, context.effects)
                .await
                .map_err(loop_tool_error)?;

            Ok(response_input_to_loop_tool_result(response))
        }

        async fn execute_prepared_streaming(
            &self,
            call: LoopToolCall,
            mut prepared: PreparedToolCall,
            context: ToolExecutionContext,
            _lifecycle: &(dyn ToolLifecycleSink + Send + Sync),
        ) -> LoopResult<LoopToolResult> {
            let preparation = prepared.take_state::<HandlerPreparation>().ok_or_else(|| {
                TurnError::new(
                    TurnErrorKind::Internal,
                    "Praxis tool plan lost its prepared handler state",
                )
            })?;
            let core_call = loop_tool_call_to_core_tool_call(call)?;
            let response = self
                .runtime
                .clone()
                .handle_prepared_tool_call_observed(
                    core_call,
                    preparation,
                    context.cancel,
                    context.effects,
                )
                .await
                .map_err(loop_tool_error)?;
            Ok(response_input_to_loop_tool_result(response))
        }
    }

    fn loop_tool_error(err: PraxisErr) -> TurnError {
        TurnError::new(TurnErrorKind::Tool, err.to_string())
    }

    fn loop_tool_spec(name: &str, spec: &praxis_tools::ToolSpec) -> LoopToolSpec {
        LoopToolSpec {
            name: name.to_string(),
            description: core_tool_description(spec),
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod tool_call_bridge {
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
                    arguments: tool_search_arguments_projection(call.arguments.as_str())
                        .into_value(),
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
}

pub(in crate::praxis::turn_loop_adapter) mod tool_result_bridge {
    mod output {
        use praxis_loop::tool::ToolResult as LoopToolResult;
        use praxis_protocol::models::ResponseInputItem;

        mod function_output {
            use praxis_loop::tool::ToolResult as LoopToolResult;
            use praxis_loop::tool::ToolResultStatus as LoopToolResultStatus;
            use praxis_protocol::models::FunctionCallOutputBody;
            use praxis_protocol::models::FunctionCallOutputPayload;

            pub(in crate::praxis::turn_loop_adapter) fn function_call_output_to_loop_result(
                call_id: String,
                output: FunctionCallOutputPayload,
            ) -> LoopToolResult {
                LoopToolResult::with_status(
                    call_id,
                    function_output_to_text(output.body),
                    LoopToolResultStatus::from_success_flag(output.success != Some(false)),
                )
            }

            fn function_output_to_text(body: FunctionCallOutputBody) -> String {
                match body {
                    FunctionCallOutputBody::Text(text) => text,
                    FunctionCallOutputBody::ContentItems(items) => {
                        praxis_protocol::models::function_call_output_content_items_to_text(&items)
                            .unwrap_or_default()
                    }
                }
            }
        }
        mod message_output {
            use praxis_loop::tool::ToolResult as LoopToolResult;
            use praxis_protocol::models::ContentItem;

            pub(in crate::praxis::turn_loop_adapter) fn non_tool_message_to_loop_result(
                content: Vec<ContentItem>,
            ) -> LoopToolResult {
                let text = content_items_to_text(content);
                LoopToolResult::error(
                    String::new(),
                    format!("tool returned non-tool message output: {text}"),
                )
            }

            fn content_items_to_text(items: Vec<ContentItem>) -> String {
                let mut parts = Vec::new();
                for item in items {
                    content_item_projection(item).append_to(&mut parts);
                }
                parts.join("\n")
            }

            enum MessageOutputProjection {
                Text(String),
                Image,
            }

            impl MessageOutputProjection {
                fn append_to(self, parts: &mut Vec<String>) {
                    match self {
                        Self::Text(text) => parts.push(text),
                        Self::Image => parts.push("[image]".to_owned()),
                    }
                }
            }

            fn content_item_projection(item: ContentItem) -> MessageOutputProjection {
                match item {
                    ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                        MessageOutputProjection::Text(text)
                    }
                    ContentItem::InputImage { .. } => MessageOutputProjection::Image,
                }
            }
        }
        mod tool_search_output {
            use praxis_loop::tool::ToolResult as LoopToolResult;

            enum SerializedToolSearchOutput {
                Json(String),
                ErrorText(String),
            }

            impl SerializedToolSearchOutput {
                fn into_content(self) -> String {
                    match self {
                        Self::Json(content) | Self::ErrorText(content) => content,
                    }
                }
            }

            pub(in crate::praxis::turn_loop_adapter) fn tool_search_output_to_loop_result(
                call_id: String,
                tools: Vec<serde_json::Value>,
            ) -> LoopToolResult {
                let content = serialize_tool_search_output(&tools).into_content();
                LoopToolResult::success(call_id, content)
            }

            fn serialize_tool_search_output(
                tools: &[serde_json::Value],
            ) -> SerializedToolSearchOutput {
                serde_json::to_string(tools).map_or_else(
                    |err| {
                        SerializedToolSearchOutput::ErrorText(format!(
                            "failed to serialize tool_search output: {err}"
                        ))
                    },
                    SerializedToolSearchOutput::Json,
                )
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn response_input_to_loop_tool_result(
            response: ResponseInputItem,
        ) -> LoopToolResult {
            match response {
                ResponseInputItem::FunctionCallOutput { call_id, output }
                | ResponseInputItem::CustomToolCallOutput {
                    call_id, output, ..
                } => function_output::function_call_output_to_loop_result(call_id, output),
                ResponseInputItem::McpToolCallOutput { call_id, output } => {
                    function_output::function_call_output_to_loop_result(
                        call_id,
                        output.as_function_call_output_payload(),
                    )
                }
                ResponseInputItem::ToolSearchOutput { call_id, tools, .. } => {
                    tool_search_output::tool_search_output_to_loop_result(call_id, tools)
                }
                ResponseInputItem::Message { content, .. } => {
                    message_output::non_tool_message_to_loop_result(content)
                }
            }
        }
    }
    mod spec {
        use praxis_tools::ToolSpec as CoreToolSpec;

        pub(in crate::praxis::turn_loop_adapter) fn core_tool_description(
            spec: &CoreToolSpec,
        ) -> String {
            match spec {
                CoreToolSpec::Function(tool) => tool.description.clone(),
                CoreToolSpec::ToolSearch { description, .. } => description.clone(),
                CoreToolSpec::LocalShell {} => "Run a local shell command".to_string(),
                CoreToolSpec::ImageGeneration { .. } => "Generate an image".to_string(),
                CoreToolSpec::WebSearch { .. } => "Search the web".to_string(),
                CoreToolSpec::Freeform(tool) => tool.description.clone(),
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter) use output::response_input_to_loop_tool_result;
    pub(in crate::praxis::turn_loop_adapter) use spec::core_tool_description;
}

pub(in crate::praxis::turn_loop_adapter) mod local_shell_bridge {
    use praxis_protocol::models::LocalShellAction;
    use praxis_protocol::models::LocalShellExecAction;
    use praxis_protocol::models::ShellToolCallParams;
    use serde_json::json;

    pub(in crate::praxis::turn_loop_adapter) fn params_to_json(
        params: &ShellToolCallParams,
    ) -> String {
        json!({
            "command": params.command.clone(),
            "workdir": params.workdir.clone(),
            "timeout_ms": params.timeout_ms,
            "sandbox_permissions": params.sandbox_permissions.clone(),
            "prefix_rule": params.prefix_rule.clone(),
            "additional_permissions": params.additional_permissions.clone(),
            "justification": params.justification.clone(),
        })
        .to_string()
    }

    enum ShellArgumentsProjection {
        Parsed(ShellToolCallParams),
        Invalid,
    }

    pub(in crate::praxis::turn_loop_adapter) fn exec_action_from_arguments(
        arguments: &str,
    ) -> LocalShellAction {
        match shell_arguments_projection(arguments) {
            ShellArgumentsProjection::Parsed(params) => {
                LocalShellAction::Exec(LocalShellExecAction {
                    command: params.command,
                    timeout_ms: params.timeout_ms,
                    working_directory: params.workdir,
                    env: None,
                    user: None,
                })
            }
            ShellArgumentsProjection::Invalid => LocalShellAction::Exec(LocalShellExecAction {
                command: Vec::new(),
                timeout_ms: None,
                working_directory: None,
                env: None,
                user: None,
            }),
        }
    }

    fn shell_arguments_projection(arguments: &str) -> ShellArgumentsProjection {
        serde_json::from_str::<ShellToolCallParams>(arguments).map_or(
            ShellArgumentsProjection::Invalid,
            ShellArgumentsProjection::Parsed,
        )
    }
}

pub(in crate::praxis::turn_loop_adapter) mod services {
    use super::super::Session;
    use super::super::TurnContext;
    use super::model_round_state::PraxisModelRoundState;
    use super::state::PraxisTurnBridgeState;
    use super::steering_decision;
    use super::tool_runtime_slot::ModelRoundToolsSlot;
    use praxis_loop::services::SteeringDrain;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    use crate::client::ModelClientSession;
    use crate::tools::context::SharedTurnDiffTracker;

    mod event_sink {
        use async_trait::async_trait;
        use praxis_loop::model::TurnEvent;
        use praxis_loop::outcome::LoopResult;
        use praxis_loop::services::EventSink;

        use super::PraxisTurnServices;
        use super::loop_event_sink_projection;

        #[async_trait]
        impl EventSink for PraxisTurnServices {
            async fn emit_event(&self, event: TurnEvent) -> LoopResult<()> {
                loop_event_sink_projection::emit_loop_event(
                    self.session(),
                    self.turn_context(),
                    self.turn_diff_tracker().await,
                    event,
                )
                .await;
                Ok(())
            }
        }
    }
    mod history {
        use async_trait::async_trait;
        use praxis_loop::model::TurnItem as LoopTurnItem;
        use praxis_loop::outcome::LoopResult;
        use praxis_loop::services::HistorySink;

        use super::super::history_bridge;
        use super::PraxisTurnServices;

        #[async_trait]
        impl HistorySink for PraxisTurnServices {
            async fn persist_items(&self, items: &[LoopTurnItem]) -> LoopResult<()> {
                let response_items = history_bridge::loop_turn_items_to_response_items(items);
                for response_item in response_items {
                    self.session
                        .record_response_item_and_emit_turn_item(&self.turn_context, response_item)
                        .await;
                }
                Ok(())
            }
        }
    }
    mod loop_event_sink_projection {
        use std::sync::Arc;

        use praxis_loop::model::TurnEvent;

        use crate::tools::context::SharedTurnDiffTracker;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::turn_event_emitter;

        pub(in crate::praxis::turn_loop_adapter) async fn emit_loop_event(
            session: Arc<Session>,
            turn_context: Arc<TurnContext>,
            turn_diff_tracker: SharedTurnDiffTracker,
            event: TurnEvent,
        ) {
            match event {
                TurnEvent::TextDelta { item_id, text } => {
                    turn_event_emitter::emit_text_delta(&session, &turn_context, item_id, text)
                        .await;
                }
                TurnEvent::ReasoningDelta {
                    item_id,
                    summary_index,
                    content_index,
                    text,
                } => {
                    turn_event_emitter::emit_reasoning_delta(
                        &session,
                        &turn_context,
                        item_id,
                        summary_index,
                        content_index,
                        text,
                    )
                    .await;
                }
                TurnEvent::ToolStarted { .. } | TurnEvent::ToolProgress { .. } => {}
                TurnEvent::ToolFinished(_) | TurnEvent::TurnCompleted => {
                    turn_event_emitter::emit_turn_diff_if_present(
                        &session,
                        &turn_context,
                        &turn_diff_tracker,
                    )
                    .await;
                }
                TurnEvent::TurnAborted(_) => {}
            }
        }
    }
    mod model_service {
        use std::sync::Arc;

        use async_trait::async_trait;
        use praxis_loop::outcome::LoopResult;
        use praxis_loop::services::ModelEventStream;
        use praxis_loop::services::ModelRequest;
        use praxis_loop::services::ModelService;
        use tokio_util::sync::CancellationToken;

        use super::super::model_stream;
        use super::super::model_stream::PraxisModelStreamInput;
        use super::PraxisTurnServices;

        #[async_trait]
        impl ModelService for PraxisTurnServices {
            async fn stream_model(
                &self,
                request: ModelRequest,
                cancellation_token: CancellationToken,
            ) -> LoopResult<ModelEventStream> {
                model_stream::stream_model(
                    PraxisModelStreamInput {
                        session: Arc::clone(&self.session),
                        turn_context: Arc::clone(&self.turn_context),
                        bridge_state: Arc::clone(&self.bridge_state),
                        runtime_state: Arc::clone(&self.runtime_state),
                        tool_runtime_slot: self.tool_runtime_slot.clone(),
                    },
                    request,
                    cancellation_token,
                )
                .await
            }
        }
    }
    mod steering {
        use async_trait::async_trait;
        use praxis_loop::outcome::LoopResult;
        use praxis_loop::services::SteeringDrain;
        use praxis_loop::services::SteeringInbox;

        use super::PraxisTurnServices;

        #[async_trait]
        impl SteeringInbox for PraxisTurnServices {
            async fn drain_steering(&self) -> LoopResult<SteeringDrain> {
                Ok(self.process_pending_input_for_round().await)
            }

            async fn wait_for_steering(&self) -> LoopResult<()> {
                self.session.wait_for_pending_steer().await;
                Ok(())
            }
        }
    }
    mod tool_access {
        use std::sync::Arc;

        use praxis_loop::services::ToolAccess;
        use praxis_loop::tool::Tool;

        use super::super::tool_bridge;
        use super::PraxisTurnServices;

        impl ToolAccess for PraxisTurnServices {
            fn resolve_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
                let runtime = self.tool_runtime_slot.current()?;
                tool_bridge::resolve_tool_from_runtime(&runtime, name)
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter) struct PraxisTurnServices {
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
        bridge_state: Arc<PraxisTurnBridgeState>,
        runtime_state: Arc<Mutex<PraxisModelRoundState>>,
        tool_runtime_slot: ModelRoundToolsSlot,
    }

    impl PraxisTurnServices {
        pub(in crate::praxis::turn_loop_adapter) fn new(
            sess: Arc<Session>,
            turn_context: Arc<TurnContext>,
            bridge_state: Arc<PraxisTurnBridgeState>,
            prewarmed_client_session: Option<ModelClientSession>,
        ) -> Self {
            let runtime_state = PraxisModelRoundState::new(
                sess.as_ref(),
                turn_context.as_ref(),
                prewarmed_client_session,
            );
            Self {
                session: sess,
                turn_context,
                bridge_state,
                runtime_state: Arc::new(Mutex::new(runtime_state)),
                tool_runtime_slot: ModelRoundToolsSlot::default(),
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn session(&self) -> Arc<Session> {
            Arc::clone(&self.session)
        }

        pub(in crate::praxis::turn_loop_adapter) fn turn_context(&self) -> Arc<TurnContext> {
            Arc::clone(&self.turn_context)
        }

        pub(in crate::praxis::turn_loop_adapter) async fn turn_diff_tracker(
            &self,
        ) -> SharedTurnDiffTracker {
            self.runtime_state.lock().await.turn_diff_tracker()
        }

        pub(in crate::praxis::turn_loop_adapter) async fn last_agent_message(
            &self,
        ) -> Option<String> {
            self.bridge_state.last_agent_message().await
        }

        pub(in crate::praxis::turn_loop_adapter) async fn process_pending_input_for_round(
            &self,
        ) -> SteeringDrain {
            steering_decision::process_pending_input_for_round(&self.session, &self.turn_context)
                .await
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod event_scope {
    use crate::util::error_or_panic;

    use super::super::Session;
    use super::super::TurnContext;

    pub(in crate::praxis::turn_loop_adapter) struct TurnEventScope {
        thread_id: String,
        turn_id: String,
    }

    impl TurnEventScope {
        pub(in crate::praxis::turn_loop_adapter) fn new(
            session: &Session,
            turn_context: &TurnContext,
        ) -> Self {
            Self {
                thread_id: session.conversation_id.to_string(),
                turn_id: turn_context.sub_id.clone(),
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn thread_id(&self) -> String {
            self.thread_id.clone()
        }

        pub(in crate::praxis::turn_loop_adapter) fn turn_id(&self) -> String {
            self.turn_id.clone()
        }

        pub(in crate::praxis::turn_loop_adapter) fn active_item_id(
            &self,
            item_id: Option<String>,
            event_name: &'static str,
        ) -> Option<String> {
            if item_id.is_some() {
                return item_id;
            }
            error_or_panic(format!("{event_name} without active item"));
            None
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod turn_event_emitter {
    mod assistant_text {
        use std::sync::Arc;

        use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
        use praxis_protocol::protocol::EventMsg;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::event_scope::TurnEventScope;

        pub(in crate::praxis::turn_loop_adapter) async fn emit_text_delta(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            item_id: Option<String>,
            text: String,
        ) {
            let scope = TurnEventScope::new(session, turn_context);
            let Some(item_id) = scope.active_item_id(item_id, "TextDelta") else {
                return;
            };

            session
                .send_event(
                    turn_context,
                    EventMsg::AgentMessageContentDelta(AgentMessageContentDeltaEvent {
                        thread_id: scope.thread_id(),
                        turn_id: scope.turn_id(),
                        item_id,
                        delta: text,
                    }),
                )
                .await;
        }
    }
    mod reasoning {
        use std::sync::Arc;

        use praxis_protocol::protocol::EventMsg;
        use praxis_protocol::protocol::ReasoningContentDeltaEvent;
        use praxis_protocol::protocol::ReasoningRawContentDeltaEvent;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::event_scope::TurnEventScope;

        pub(in crate::praxis::turn_loop_adapter) async fn emit_reasoning_delta(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            item_id: Option<String>,
            summary_index: Option<i64>,
            content_index: Option<i64>,
            text: String,
        ) {
            let scope = TurnEventScope::new(session, turn_context);
            let Some(item_id) = scope.active_item_id(item_id, "ReasoningDelta") else {
                return;
            };

            if let Some(content_index) = content_index {
                session
                    .send_event(
                        turn_context,
                        EventMsg::ReasoningRawContentDelta(ReasoningRawContentDeltaEvent {
                            thread_id: scope.thread_id(),
                            turn_id: scope.turn_id(),
                            item_id,
                            delta: text,
                            content_index,
                        }),
                    )
                    .await;
                return;
            }

            session
                .send_event(
                    turn_context,
                    EventMsg::ReasoningContentDelta(ReasoningContentDeltaEvent {
                        thread_id: scope.thread_id(),
                        turn_id: scope.turn_id(),
                        item_id,
                        delta: text,
                        summary_index: summary_index.unwrap_or_default(),
                    }),
                )
                .await;
        }
    }
    mod turn_diff {
        use std::sync::Arc;

        use praxis_protocol::protocol::EventMsg;
        use praxis_protocol::protocol::TurnDiffEvent;

        use crate::tools::context::SharedTurnDiffTracker;

        use super::super::super::Session;
        use super::super::super::TurnContext;

        pub(in crate::praxis::turn_loop_adapter) async fn emit_turn_diff_if_present(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            turn_diff_tracker: &SharedTurnDiffTracker,
        ) {
            let unified_diff = {
                let mut tracker = turn_diff_tracker.lock().await;
                tracker.get_unified_diff()
            };

            if let Ok(Some(unified_diff)) = unified_diff {
                session
                    .send_event(
                        turn_context,
                        EventMsg::TurnDiff(TurnDiffEvent { unified_diff }),
                    )
                    .await;
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter) use assistant_text::emit_text_delta;
    pub(in crate::praxis::turn_loop_adapter) use reasoning::emit_reasoning_delta;
    pub(in crate::praxis::turn_loop_adapter) use turn_diff::emit_turn_diff_if_present;
}

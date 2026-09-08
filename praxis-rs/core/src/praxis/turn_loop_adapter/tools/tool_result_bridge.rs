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

        fn serialize_tool_search_output(tools: &[serde_json::Value]) -> SerializedToolSearchOutput {
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

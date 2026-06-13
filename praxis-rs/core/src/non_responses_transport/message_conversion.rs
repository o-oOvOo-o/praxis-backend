use super::*;

pub(super) fn response_item_to_claude_message(item: &ResponseItem) -> Option<Value> {
    match item {
        ResponseItem::Message { role, content, .. } if role == "user" || role == "assistant" => {
            let blocks = claude_content_blocks(content);
            (!blocks.is_empty()).then_some(json!({
                "role": role,
                "content": blocks,
            }))
        }
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            ..
        } => Some(json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": call_id,
                "name": name,
                "input": normalize_function_arguments(arguments),
            }],
        })),
        ResponseItem::CustomToolCall {
            name,
            input,
            call_id,
            ..
        } => Some(json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": call_id,
                "name": name,
                "input": { "input": input },
            }],
        })),
        ResponseItem::LocalShellCall {
            call_id,
            id,
            action: LocalShellAction::Exec(exec),
            ..
        } => Some(json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": call_id.clone().or_else(|| id.clone()).unwrap_or_else(|| Uuid::new_v4().to_string()),
                "name": "local_shell",
                "input": local_shell_exec_json(exec),
            }],
        })),
        ResponseItem::ToolSearchCall {
            call_id: Some(call_id),
            execution,
            arguments,
            ..
        } if execution == "client" => Some(json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": call_id,
                "name": "tool_search",
                "input": arguments,
            }],
        })),
        ResponseItem::FunctionCallOutput { call_id, output } => {
            Some(tool_result_to_claude_message(call_id, None, output))
        }
        ResponseItem::CustomToolCallOutput {
            call_id,
            name,
            output,
        } => Some(tool_result_to_claude_message(
            call_id,
            name.as_deref(),
            output,
        )),
        ResponseItem::ToolSearchOutput {
            call_id: Some(call_id),
            status,
            execution,
            tools,
        } => Some(json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": call_id,
                "content": serde_json::to_string(&json!({
                    "status": status,
                    "execution": execution,
                    "tools": tools,
                })).unwrap_or_default(),
            }],
        })),
        _ => None,
    }
}

pub(super) fn response_item_to_common_message(
    item: &ResponseItem,
    compat: &CommonRequestCompat,
    tool_names_by_call_id: &mut BTreeMap<String, String>,
) -> Option<(Value, Option<CommonHistoryRole>)> {
    match item {
        ResponseItem::Message { role, content, .. } if role == "system" => {
            let rendered = render_text_only_content(content);
            (!rendered.trim().is_empty()).then_some((
                json!({
                    "role": "system",
                    "content": rendered,
                }),
                None,
            ))
        }
        ResponseItem::Message { role, content, .. }
            if role == "developer" && compat.supports_developer_role =>
        {
            let rendered = render_text_only_content(content);
            (!rendered.trim().is_empty()).then_some((
                json!({
                    "role": "developer",
                    "content": rendered,
                }),
                None,
            ))
        }
        ResponseItem::Message { role, content, .. } if role == "user" || role == "assistant" => {
            Some((
                json!({
                    "role": role,
                    "content": common_message_content(content),
                }),
                Some(if role == "user" {
                    CommonHistoryRole::User
                } else {
                    CommonHistoryRole::Assistant
                }),
            ))
        }
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            provider_metadata,
            ..
        } => {
            tool_names_by_call_id.insert(call_id.clone(), name.clone());
            let mut tool_call = json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": normalize_function_arguments_string(arguments),
                }
            });
            if compat.preserve_tool_call_provider_metadata {
                merge_common_tool_call_provider_metadata(
                    &mut tool_call,
                    provider_metadata.as_ref(),
                );
            }
            Some((
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [tool_call],
                }),
                Some(CommonHistoryRole::Assistant),
            ))
        }
        ResponseItem::CustomToolCall {
            name,
            input,
            call_id,
            ..
        } => {
            tool_names_by_call_id.insert(call_id.clone(), name.clone());
            Some((
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(&json!({ "input": input })).unwrap_or_default(),
                        }
                    }],
                }),
                Some(CommonHistoryRole::Assistant),
            ))
        }
        ResponseItem::LocalShellCall {
            call_id,
            id,
            action: LocalShellAction::Exec(exec),
            ..
        } => {
            let tool_call_id = call_id
                .clone()
                .or_else(|| id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            tool_names_by_call_id.insert(tool_call_id.clone(), "local_shell".to_string());
            Some((
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": tool_call_id,
                        "type": "function",
                        "function": {
                            "name": "local_shell",
                            "arguments": serde_json::to_string(&local_shell_exec_json(exec)).unwrap_or_default(),
                        }
                    }],
                }),
                Some(CommonHistoryRole::Assistant),
            ))
        }
        ResponseItem::ToolSearchCall {
            call_id: Some(call_id),
            execution,
            arguments,
            ..
        } if execution == "client" => {
            tool_names_by_call_id.insert(call_id.clone(), "tool_search".to_string());
            Some((
                json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": "tool_search",
                            "arguments": serde_json::to_string(arguments).unwrap_or_default(),
                        }
                    }],
                }),
                Some(CommonHistoryRole::Assistant),
            ))
        }
        ResponseItem::FunctionCallOutput { call_id, output } => Some((
            common_tool_result_message(
                call_id,
                output,
                compat,
                tool_names_by_call_id.get(call_id).map(String::as_str),
            ),
            Some(CommonHistoryRole::ToolResult),
        )),
        ResponseItem::CustomToolCallOutput {
            call_id,
            name,
            output,
        } => Some((
            common_tool_result_message(
                call_id,
                output,
                compat,
                name.as_deref()
                    .or_else(|| tool_names_by_call_id.get(call_id).map(String::as_str)),
            ),
            Some(CommonHistoryRole::ToolResult),
        )),
        ResponseItem::ToolSearchOutput {
            call_id: Some(call_id),
            status,
            execution,
            tools,
        } => Some((
            common_tool_result_message_from_string(
                call_id,
                serde_json::to_string(&json!({
                    "status": status,
                    "execution": execution,
                    "tools": tools,
                }))
                .unwrap_or_default(),
                compat,
                tool_names_by_call_id
                    .get(call_id)
                    .map(String::as_str)
                    .or(Some("tool_search")),
            ),
            Some(CommonHistoryRole::ToolResult),
        )),
        _ => None,
    }
}

pub(super) fn response_item_is_common_tool_call(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::FunctionCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::LocalShellCall { .. } => true,
        ResponseItem::ToolSearchCall {
            execution,
            call_id: Some(_),
            ..
        } => execution == "client",
        _ => false,
    }
}

pub(super) fn response_item_to_common_tool_call(
    item: &ResponseItem,
    compat: &CommonRequestCompat,
    tool_names_by_call_id: &mut BTreeMap<String, String>,
) -> Option<Value> {
    match item {
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            provider_metadata,
            ..
        } => {
            tool_names_by_call_id.insert(call_id.clone(), name.clone());
            let mut tool_call = json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": normalize_function_arguments_string(arguments),
                }
            });
            if compat.preserve_tool_call_provider_metadata {
                merge_common_tool_call_provider_metadata(
                    &mut tool_call,
                    provider_metadata.as_ref(),
                );
            }
            Some(tool_call)
        }
        ResponseItem::CustomToolCall {
            name,
            input,
            call_id,
            ..
        } => {
            tool_names_by_call_id.insert(call_id.clone(), name.clone());
            Some(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": serde_json::to_string(&json!({ "input": input })).unwrap_or_default(),
                }
            }))
        }
        ResponseItem::LocalShellCall {
            call_id,
            id,
            action: LocalShellAction::Exec(exec),
            ..
        } => {
            let tool_call_id = call_id
                .clone()
                .or_else(|| id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            tool_names_by_call_id.insert(tool_call_id.clone(), "local_shell".to_string());
            Some(json!({
                "id": tool_call_id,
                "type": "function",
                "function": {
                    "name": "local_shell",
                    "arguments": serde_json::to_string(&local_shell_exec_json(exec)).unwrap_or_default(),
                }
            }))
        }
        ResponseItem::ToolSearchCall {
            call_id: Some(call_id),
            execution,
            arguments,
            ..
        } if execution == "client" => {
            tool_names_by_call_id.insert(call_id.clone(), "tool_search".to_string());
            Some(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": "tool_search",
                    "arguments": serde_json::to_string(arguments).unwrap_or_default(),
                }
            }))
        }
        _ => None,
    }
}

pub(super) fn common_history_role(item: &ResponseItem) -> Option<CommonHistoryRole> {
    match item {
        ResponseItem::Message { role, .. } if role == "user" => Some(CommonHistoryRole::User),
        ResponseItem::Message { role, .. } if role == "assistant" => {
            Some(CommonHistoryRole::Assistant)
        }
        ResponseItem::FunctionCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::LocalShellCall { .. } => Some(CommonHistoryRole::Assistant),
        ResponseItem::ToolSearchCall { execution, .. } if execution == "client" => {
            Some(CommonHistoryRole::Assistant)
        }
        ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::ToolSearchOutput { .. } => Some(CommonHistoryRole::ToolResult),
        _ => None,
    }
}

pub(super) fn common_tool_result_message(
    call_id: &str,
    output: &FunctionCallOutputPayload,
    compat: &CommonRequestCompat,
    tool_name: Option<&str>,
) -> Value {
    common_tool_result_message_from_string(call_id, output.to_string(), compat, tool_name)
}

pub(super) fn common_tool_result_message_from_string(
    call_id: &str,
    content: String,
    compat: &CommonRequestCompat,
    tool_name: Option<&str>,
) -> Value {
    let mut message = serde_json::Map::from_iter([
        ("role".to_string(), Value::String("tool".to_string())),
        (
            "tool_call_id".to_string(),
            Value::String(call_id.to_string()),
        ),
        ("content".to_string(), Value::String(content)),
    ]);
    if compat.requires_tool_result_name
        && let Some(name) = tool_name
    {
        message.insert("name".to_string(), Value::String(name.to_string()));
    }
    Value::Object(message)
}

pub(super) fn apply_common_reasoning_config(
    request: &mut serde_json::Map<String, Value>,
    compat: &CommonRequestCompat,
    effort: Option<ReasoningEffortConfig>,
) {
    let policy = CommonThinkingPolicy::from_format(compat.thinking_format);
    match policy.request_style {
        CommonThinkingRequestStyle::ReasoningEffortField => {
            if compat.supports_reasoning_effort
                && let Some(effort) = effort
            {
                request.insert(
                    "reasoning_effort".to_string(),
                    Value::String(map_common_reasoning_effort(
                        effort,
                        compat.reasoning_effort_map.as_ref(),
                    )),
                );
            }
        }
        CommonThinkingRequestStyle::OpenRouterReasoningObject => {
            if let Some(effort) = effort {
                request.insert(
                    "reasoning".to_string(),
                    json!({
                        "effort": map_common_reasoning_effort(
                            effort,
                            compat.reasoning_effort_map.as_ref(),
                        ),
                    }),
                );
            }
        }
        CommonThinkingRequestStyle::EnableThinkingBool => {
            request.insert(
                "enable_thinking".to_string(),
                Value::Bool(reasoning_effort_enables_thinking(effort)),
            );
        }
        CommonThinkingRequestStyle::ZaiThinkingObject => {
            let thinking_type = if reasoning_effort_enables_thinking(effort) {
                "enabled"
            } else {
                "disabled"
            };
            request.insert("thinking".to_string(), json!({ "type": thinking_type }));
        }
        CommonThinkingRequestStyle::QwenChatTemplateKwargs => {
            request.insert(
                "chat_template_kwargs".to_string(),
                json!({
                    "enable_thinking": reasoning_effort_enables_thinking(effort),
                }),
            );
        }
    }
}

pub(super) fn map_common_reasoning_effort(
    effort: ReasoningEffortConfig,
    mapping: Option<&ModelProviderReasoningEffortMap>,
) -> String {
    match effort {
        ReasoningEffortConfig::None => "none".to_string(),
        ReasoningEffortConfig::Minimal => mapping
            .and_then(|mapping| mapping.minimal.clone())
            .unwrap_or_else(|| effort.to_string()),
        ReasoningEffortConfig::Low => mapping
            .and_then(|mapping| mapping.low.clone())
            .unwrap_or_else(|| effort.to_string()),
        ReasoningEffortConfig::Medium => mapping
            .and_then(|mapping| mapping.medium.clone())
            .unwrap_or_else(|| effort.to_string()),
        ReasoningEffortConfig::High => mapping
            .and_then(|mapping| mapping.high.clone())
            .unwrap_or_else(|| effort.to_string()),
        ReasoningEffortConfig::XHigh => mapping
            .and_then(|mapping| mapping.xhigh.clone())
            .unwrap_or_else(|| effort.to_string()),
    }
}

pub(super) fn reasoning_effort_enables_thinking(effort: Option<ReasoningEffortConfig>) -> bool {
    matches!(
        effort,
        Some(reasoning_effort) if reasoning_effort != ReasoningEffortConfig::None
    )
}

pub(super) fn common_max_tokens_field_name(field: ModelProviderMaxTokensField) -> &'static str {
    match field {
        ModelProviderMaxTokensField::MaxCompletionTokens => "max_completion_tokens",
        ModelProviderMaxTokensField::MaxTokens => "max_tokens",
    }
}

pub(super) fn tool_result_to_claude_message(
    call_id: &str,
    name: Option<&str>,
    output: &FunctionCallOutputPayload,
) -> Value {
    let mut block = serde_json::Map::from_iter([
        ("type".to_string(), Value::String("tool_result".to_string())),
        (
            "tool_use_id".to_string(),
            Value::String(call_id.to_string()),
        ),
        ("content".to_string(), Value::String(output.to_string())),
    ]);
    if let Some(false) = output.success {
        block.insert("is_error".to_string(), Value::Bool(true));
    }
    if let Some(name) = name {
        block.insert("name".to_string(), Value::String(name.to_string()));
    }
    json!({
        "role": "user",
        "content": [Value::Object(block)],
    })
}

pub(super) fn claude_content_blocks(content: &[ContentItem]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.is_empty() {
                    blocks.push(json!({
                        "type": "text",
                        "text": text,
                    }));
                }
            }
            ContentItem::InputImage { image_url } => {
                if let Some((media_type, data)) = parse_data_url(image_url) {
                    blocks.push(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data,
                        }
                    }));
                } else {
                    blocks.push(json!({
                        "type": "text",
                        "text": format!("[Image URL: {image_url}]"),
                    }));
                }
            }
        }
    }
    blocks
}

pub(super) fn common_message_content(content: &[ContentItem]) -> Value {
    if !content
        .iter()
        .any(|item| matches!(item, ContentItem::InputImage { .. }))
    {
        return Value::String(render_text_only_content(content));
    }

    Value::Array(
        content
            .iter()
            .map(|item| match item {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => json!({
                    "type": "text",
                    "text": text,
                }),
                ContentItem::InputImage { image_url } => json!({
                    "type": "image_url",
                    "image_url": { "url": image_url },
                }),
            })
            .collect(),
    )
}

pub(super) fn render_text_only_content(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                Some(text.as_str())
            }
            ContentItem::InputImage { image_url } => Some(image_url.as_str()),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn parse_data_url(image_url: &str) -> Option<(String, String)> {
    let image_url = image_url.strip_prefix("data:")?;
    let (metadata, data) = image_url.split_once(",")?;
    let media_type = metadata.strip_suffix(";base64")?;
    Some((media_type.to_string(), data.to_string()))
}

pub(super) fn tool_spec_to_claude_tool(tool: &ToolSpec) -> Option<Value> {
    let function = tool_spec_to_function_definition(tool)?;
    Some(json!({
        "name": function.name,
        "description": function.description,
        "input_schema": function.parameters,
    }))
}

pub(super) fn tool_spec_to_common_tool(tool: &ToolSpec) -> Option<Value> {
    let function = tool_spec_to_function_definition(tool)?;
    Some(json!({
        "type": "function",
        "function": {
            "name": function.name,
            "description": function.description,
            "parameters": function.parameters,
        }
    }))
}

pub(super) struct ProviderFunctionTool {
    name: String,
    description: String,
    parameters: Value,
}

pub(super) fn tool_spec_to_function_definition(tool: &ToolSpec) -> Option<ProviderFunctionTool> {
    match tool {
        ToolSpec::Function(tool) => Some(ProviderFunctionTool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: serde_json::to_value(&tool.parameters).ok()?,
        }),
        ToolSpec::ToolSearch {
            description,
            parameters,
            ..
        } => Some(ProviderFunctionTool {
            name: "tool_search".to_string(),
            description: description.clone(),
            parameters: serde_json::to_value(parameters).ok()?,
        }),
        ToolSpec::LocalShell {} => Some(ProviderFunctionTool {
            name: "local_shell".to_string(),
            description: "Execute a local shell command on the user's machine.".to_string(),
            parameters: serde_json::to_value(local_shell_schema()).ok()?,
        }),
        ToolSpec::Freeform(tool) => Some(ProviderFunctionTool {
            name: tool.name.clone(),
            description: format!(
                "{}\n\nPass the full raw tool input string in the `input` field.",
                tool.description
            ),
            parameters: serde_json::to_value(freeform_tool_schema()).ok()?,
        }),
        ToolSpec::WebSearch { .. } => Some(ProviderFunctionTool {
            name: "web_search".to_string(),
            description: "Search the public web with Praxis zero-config parallel web search. Praxis fans out to every no-key provider it can reach, including Google, Bing, DuckDuckGo HTML, Startpage, Baidu, GitHub, docs.rs, crates.io, and local YaCy, then deduplicates and ranks all successful provider results while reporting failures.".to_string(),
            parameters: serde_json::to_value(web_search_schema()).ok()?,
        }),
        ToolSpec::ImageGeneration { .. } => None,
    }
}

pub(super) fn local_shell_schema() -> JsonSchema {
    use std::collections::BTreeMap;

    JsonSchema::Object {
        properties: BTreeMap::from([
            (
                "command".to_string(),
                JsonSchema::Array {
                    items: Box::new(JsonSchema::String {
                        description: Some("A single shell argument.".to_string()),
                    }),
                    description: Some("Command argv vector to execute.".to_string()),
                },
            ),
            (
                "workdir".to_string(),
                JsonSchema::String {
                    description: Some("Optional working directory.".to_string()),
                },
            ),
            (
                "timeout_ms".to_string(),
                JsonSchema::Number {
                    description: Some("Optional timeout in milliseconds.".to_string()),
                },
            ),
        ]),
        required: Some(vec!["command".to_string()]),
        additional_properties: Some(false.into()),
    }
}

pub(super) fn freeform_tool_schema() -> JsonSchema {
    use std::collections::BTreeMap;

    JsonSchema::Object {
        properties: BTreeMap::from([(
            "input".to_string(),
            JsonSchema::String {
                description: Some("Full freeform tool input.".to_string()),
            },
        )]),
        required: Some(vec!["input".to_string()]),
        additional_properties: Some(false.into()),
    }
}

pub(super) fn web_search_schema() -> JsonSchema {
    use std::collections::BTreeMap;

    JsonSchema::Object {
        properties: BTreeMap::from([
            (
                "query".to_string(),
                JsonSchema::String {
                    description: Some("Primary web search query.".to_string()),
                },
            ),
            (
                "queries".to_string(),
                JsonSchema::Array {
                    items: Box::new(JsonSchema::String {
                        description: Some("Additional search query.".to_string()),
                    }),
                    description: Some(
                        "Optional alternate queries; Praxis searches every query in parallel."
                            .to_string(),
                    ),
                },
            ),
            (
                "max_results".to_string(),
                JsonSchema::Number {
                    description: Some(
                        "Maximum merged results to return, capped by Praxis.".to_string(),
                    ),
                },
            ),
            (
                "domains".to_string(),
                JsonSchema::Array {
                    items: Box::new(JsonSchema::String {
                        description: Some("Allowed result domain such as github.com.".to_string()),
                    }),
                    description: Some("Optional domain allow-list for returned results.".to_string()),
                },
            ),
            (
                "recency_days".to_string(),
                JsonSchema::Number {
                    description: Some(
                        "Optional freshness preference in days. Zero-config HTML providers may not all support strict freshness filtering.".to_string(),
                    ),
                },
            ),
        ]),
        required: Some(vec!["query".to_string()]),
        additional_properties: Some(false.into()),
    }
}

pub(super) fn local_shell_exec_json(exec: &praxis_protocol::models::LocalShellExecAction) -> Value {
    let mut object = serde_json::Map::from_iter([(
        "command".to_string(),
        Value::Array(exec.command.iter().cloned().map(Value::String).collect()),
    )]);

    if let Some(workdir) = &exec.working_directory {
        object.insert("workdir".to_string(), Value::String(workdir.clone()));
    }
    if let Some(timeout_ms) = exec.timeout_ms {
        object.insert("timeout_ms".to_string(), Value::Number(timeout_ms.into()));
    }

    Value::Object(object)
}

pub(super) fn normalize_function_arguments(arguments: &str) -> Value {
    match serde_json::from_str::<Value>(arguments) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(other) => json!({ "value": other }),
        Err(_) => json!({ "input": arguments }),
    }
}

pub(super) fn normalize_function_arguments_string(arguments: &str) -> String {
    match serde_json::from_str::<Value>(arguments) {
        Ok(value) => serde_json::to_string(&value).unwrap_or_else(|_| arguments.to_string()),
        Err(_) => serde_json::to_string(&json!({ "input": arguments }))
            .unwrap_or_else(|_| arguments.to_string()),
    }
}

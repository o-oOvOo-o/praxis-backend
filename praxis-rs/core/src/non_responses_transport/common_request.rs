use super::*;

pub(super) fn build_claude_request(
    prompt: &Prompt,
    model_info: &ModelInfo,
    stream: bool,
) -> Result<Value> {
    let formatted_input = prompt.get_formatted_input();
    let system = collect_system_prompt(prompt, &formatted_input);
    let messages = formatted_input
        .iter()
        .filter_map(response_item_to_claude_message)
        .collect::<Vec<_>>();
    let tools = prompt
        .tools
        .iter()
        .filter_map(tool_spec_to_claude_tool)
        .collect::<Vec<_>>();

    let mut request = serde_json::Map::from_iter([
        ("model".to_string(), Value::String(model_info.slug.clone())),
        (
            "max_tokens".to_string(),
            Value::Number(DEFAULT_CLAUDE_MAX_TOKENS.into()),
        ),
        ("messages".to_string(), Value::Array(messages)),
        ("stream".to_string(), Value::Bool(stream)),
    ]);

    if !system.is_empty() {
        request.insert("system".to_string(), Value::String(system));
    }
    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
    }

    Ok(Value::Object(request))
}

pub(super) fn build_common_request(
    prompt: &Prompt,
    model_info: &ModelInfo,
    provider_info: &ModelProviderInfo,
    effort: Option<ReasoningEffortConfig>,
    stream: bool,
) -> Result<Value> {
    let formatted_input = prompt.get_formatted_input();
    let compat = CommonRequestCompat::from_provider_and_model(provider_info, model_info);
    let messages = build_common_messages(prompt, &formatted_input, &compat);

    let tools = prompt
        .tools
        .iter()
        .filter_map(tool_spec_to_common_tool)
        .collect::<Vec<_>>();

    let mut request = serde_json::Map::from_iter([
        ("model".to_string(), Value::String(model_info.slug.clone())),
        ("messages".to_string(), Value::Array(messages)),
        ("stream".to_string(), Value::Bool(stream)),
    ]);

    apply_common_reasoning_config(
        &mut request,
        &compat,
        effort.or(model_info.default_reasoning_level),
    );

    if let Some(max_tokens_field) = compat.max_tokens_field {
        request.insert(
            common_max_tokens_field_name(max_tokens_field).to_string(),
            Value::Number(DEFAULT_CLAUDE_MAX_TOKENS.into()),
        );
    }

    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
        if compat.emit_parallel_tool_calls {
            request.insert(
                "parallel_tool_calls".to_string(),
                Value::Bool(prompt.parallel_tool_calls),
            );
        }
    }

    Ok(Value::Object(request))
}

#[derive(Debug, Clone)]
pub(super) struct CommonRequestCompat {
    pub(super) supports_developer_role: bool,
    pub(super) supports_reasoning_effort: bool,
    pub(super) reasoning_effort_map: Option<ModelProviderReasoningEffortMap>,
    pub(super) max_tokens_field: Option<ModelProviderMaxTokensField>,
    pub(super) thinking_format: ModelProviderThinkingFormat,
    pub(super) preserve_tool_call_provider_metadata: bool,
    pub(super) requires_tool_result_name: bool,
    pub(super) requires_assistant_after_tool_result: bool,
    pub(super) emit_parallel_tool_calls: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommonThinkingRequestStyle {
    ReasoningEffortField,
    OpenRouterReasoningObject,
    EnableThinkingBool,
    ZaiThinkingObject,
    QwenChatTemplateKwargs,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CommonThinkingPolicy {
    pub(super) request_style: CommonThinkingRequestStyle,
    pub(super) replay_field: Option<&'static str>,
    pub(super) response_fields: &'static [&'static str],
    pub(super) complete_on_message_idle: bool,
    pub(super) complete_on_finish_reason: bool,
}

impl CommonThinkingPolicy {
    pub(super) fn from_format(format: ModelProviderThinkingFormat) -> Self {
        match format {
            ModelProviderThinkingFormat::Openai => Self {
                request_style: CommonThinkingRequestStyle::ReasoningEffortField,
                replay_field: Some("reasoning_content"),
                response_fields: &["reasoning_content", "reasoning"],
                complete_on_message_idle: false,
                complete_on_finish_reason: false,
            },
            ModelProviderThinkingFormat::Openrouter => Self {
                request_style: CommonThinkingRequestStyle::OpenRouterReasoningObject,
                replay_field: None,
                response_fields: &["reasoning", "reasoning_content"],
                complete_on_message_idle: false,
                complete_on_finish_reason: false,
            },
            ModelProviderThinkingFormat::Deepseek => Self {
                request_style: CommonThinkingRequestStyle::ReasoningEffortField,
                replay_field: None,
                response_fields: &["reasoning_content", "reasoning"],
                complete_on_message_idle: true,
                complete_on_finish_reason: true,
            },
            ModelProviderThinkingFormat::Gemini => Self {
                request_style: CommonThinkingRequestStyle::ReasoningEffortField,
                replay_field: Some("reasoning_content"),
                response_fields: &["reasoning_content", "reasoning"],
                complete_on_message_idle: false,
                complete_on_finish_reason: false,
            },
            ModelProviderThinkingFormat::Zai => Self {
                request_style: CommonThinkingRequestStyle::ZaiThinkingObject,
                replay_field: Some("reasoning_content"),
                response_fields: &["reasoning_content", "reasoning"],
                complete_on_message_idle: false,
                complete_on_finish_reason: false,
            },
            ModelProviderThinkingFormat::Qwen => Self {
                request_style: CommonThinkingRequestStyle::EnableThinkingBool,
                replay_field: Some("reasoning_content"),
                response_fields: &["reasoning_content", "reasoning"],
                complete_on_message_idle: false,
                complete_on_finish_reason: false,
            },
            ModelProviderThinkingFormat::QwenChatTemplate => Self {
                request_style: CommonThinkingRequestStyle::QwenChatTemplateKwargs,
                replay_field: Some("reasoning_content"),
                response_fields: &["reasoning_content", "reasoning"],
                complete_on_message_idle: false,
                complete_on_finish_reason: false,
            },
        }
    }
}

impl Default for CommonRequestCompat {
    fn default() -> Self {
        Self {
            supports_developer_role: false,
            supports_reasoning_effort: false,
            reasoning_effort_map: None,
            max_tokens_field: None,
            thinking_format: ModelProviderThinkingFormat::Openai,
            preserve_tool_call_provider_metadata: false,
            requires_tool_result_name: false,
            requires_assistant_after_tool_result: false,
            emit_parallel_tool_calls: true,
        }
    }
}

impl CommonRequestCompat {
    pub(super) fn from_provider_and_model(
        provider_info: &ModelProviderInfo,
        model_info: &ModelInfo,
    ) -> Self {
        let inferred_compat = infer_common_request_compat(
            provider_info.base_url.as_deref(),
            model_info.slug.as_str(),
        );
        let compat = Some(merge_common_request_compat(
            inferred_compat,
            provider_info.compat.clone(),
        ));
        let compat = compat.as_ref();
        let thinking_format = compat
            .and_then(|compat| compat.thinking_format)
            .unwrap_or(ModelProviderThinkingFormat::Openai);
        Self {
            supports_developer_role: compat
                .and_then(|compat| compat.supports_developer_role)
                .unwrap_or(false),
            supports_reasoning_effort: compat
                .and_then(|compat| compat.supports_reasoning_effort)
                .unwrap_or(false),
            reasoning_effort_map: compat.and_then(|compat| compat.reasoning_effort_map.clone()),
            max_tokens_field: compat.and_then(|compat| compat.max_tokens_field),
            thinking_format,
            preserve_tool_call_provider_metadata: matches!(
                thinking_format,
                ModelProviderThinkingFormat::Gemini
            ),
            requires_tool_result_name: compat
                .and_then(|compat| compat.requires_tool_result_name)
                .unwrap_or(false),
            requires_assistant_after_tool_result: compat
                .and_then(|compat| compat.requires_assistant_after_tool_result)
                .unwrap_or(false),
            emit_parallel_tool_calls: compat
                .and_then(|compat| compat.supports_parallel_tool_calls)
                .unwrap_or(true),
        }
    }
}

pub(super) fn infer_common_request_compat(
    base_url: Option<&str>,
    model_slug: &str,
) -> crate::model_provider_info::ModelProviderCompatInfo {
    let mut compat = crate::model_provider_info::ModelProviderCompatInfo::default();
    if let Some(base_url) = base_url {
        let lower = base_url.to_ascii_lowercase();
        let is_non_openai = !lower.contains("api.openai.com");
        if is_non_openai {
            compat.supports_developer_role = Some(false);
            compat.supports_reasoning_effort = Some(true);
        }
        if lower.contains("openrouter.ai") {
            compat.thinking_format = Some(ModelProviderThinkingFormat::Openrouter);
        }
        if lower.contains("deepseek.com") {
            compat.thinking_format = Some(ModelProviderThinkingFormat::Deepseek);
        }
        if lower.contains("generativelanguage.googleapis.com")
            || lower.contains("aiplatform.googleapis.com")
        {
            compat.thinking_format = Some(ModelProviderThinkingFormat::Gemini);
        }
        if lower.contains("api.x.ai") {
            compat.supports_reasoning_effort = Some(false);
        }
        if lower.contains("bigmodel.cn") || lower.contains("z.ai") {
            compat.supports_reasoning_effort = Some(false);
            compat.max_tokens_field = Some(ModelProviderMaxTokensField::MaxTokens);
            compat.thinking_format = Some(ModelProviderThinkingFormat::Zai);
        }
    }

    let model_slug = model_slug.trim().to_ascii_lowercase();
    if model_slug.starts_with("glm-") {
        compat.thinking_format = Some(ModelProviderThinkingFormat::Zai);
    }
    if model_slug.starts_with("gemini-") {
        compat.thinking_format = Some(ModelProviderThinkingFormat::Gemini);
    }
    compat
}

pub(super) fn merge_common_request_compat(
    mut inferred: crate::model_provider_info::ModelProviderCompatInfo,
    explicit: Option<crate::model_provider_info::ModelProviderCompatInfo>,
) -> crate::model_provider_info::ModelProviderCompatInfo {
    let Some(explicit) = explicit else {
        return inferred;
    };
    inferred.supports_developer_role = explicit
        .supports_developer_role
        .or(inferred.supports_developer_role);
    inferred.supports_reasoning_effort = explicit
        .supports_reasoning_effort
        .or(inferred.supports_reasoning_effort);
    inferred.reasoning_effort_map = explicit
        .reasoning_effort_map
        .or(inferred.reasoning_effort_map);
    inferred.supports_parallel_tool_calls = explicit
        .supports_parallel_tool_calls
        .or(inferred.supports_parallel_tool_calls);
    inferred.max_tokens_field = explicit.max_tokens_field.or(inferred.max_tokens_field);
    inferred.requires_tool_result_name = explicit
        .requires_tool_result_name
        .or(inferred.requires_tool_result_name);
    inferred.requires_assistant_after_tool_result = explicit
        .requires_assistant_after_tool_result
        .or(inferred.requires_assistant_after_tool_result);
    inferred.thinking_format = explicit.thinking_format.or(inferred.thinking_format);
    inferred
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommonHistoryRole {
    User,
    Assistant,
    ToolResult,
}

pub(super) fn build_common_messages(
    prompt: &Prompt,
    formatted_input: &[ResponseItem],
    compat: &CommonRequestCompat,
) -> Vec<Value> {
    let mut messages = Vec::new();
    if compat.supports_developer_role {
        push_common_text_message(
            &mut messages,
            "developer",
            prompt.base_instructions.text.as_str(),
        );
    } else {
        let system = collect_system_prompt(prompt, formatted_input);
        push_common_text_message(&mut messages, "system", &system);
    }

    let mut tool_names_by_call_id = BTreeMap::<String, String>::new();
    let mut pending_assistant_message = None::<Value>;
    let mut pending_reasoning_content = String::new();
    for (index, item) in formatted_input.iter().enumerate() {
        if let Some(reasoning_content) = common_reasoning_content(item) {
            append_common_reasoning_content(&mut pending_reasoning_content, &reasoning_content);
            continue;
        }

        if let Some(tool_call) =
            response_item_to_common_tool_call(item, compat, &mut tool_names_by_call_id)
        {
            let message = ensure_common_assistant_message(&mut pending_assistant_message);
            attach_common_reasoning_content(message, &mut pending_reasoning_content, compat);
            append_common_tool_call(message, tool_call);
            continue;
        }

        let Some((mut message, role)) =
            response_item_to_common_message(item, compat, &mut tool_names_by_call_id)
        else {
            continue;
        };

        if role == Some(CommonHistoryRole::Assistant) {
            flush_common_assistant_message(&mut messages, &mut pending_assistant_message);
            attach_common_reasoning_content(&mut message, &mut pending_reasoning_content, compat);
            pending_assistant_message = Some(message);
            if !next_common_item_is_tool_call(&formatted_input[index + 1..]) {
                flush_common_assistant_message(&mut messages, &mut pending_assistant_message);
            }
            continue;
        }

        flush_common_assistant_message(&mut messages, &mut pending_assistant_message);
        messages.push(message);

        if compat.requires_assistant_after_tool_result
            && role == Some(CommonHistoryRole::ToolResult)
            && next_common_history_role(&formatted_input[index + 1..])
                == Some(CommonHistoryRole::User)
        {
            messages.push(json!({
                "role": "assistant",
                "content": COMMON_TOOL_RESULT_BRIDGE_MESSAGE,
            }));
        }
    }
    flush_common_assistant_message(&mut messages, &mut pending_assistant_message);

    messages
}

pub(super) fn push_common_text_message(messages: &mut Vec<Value>, role: &str, content: &str) {
    let content = content.trim();
    if content.is_empty() {
        return;
    }

    messages.push(json!({
        "role": role,
        "content": content,
    }));
}

pub(super) fn next_common_history_role(items: &[ResponseItem]) -> Option<CommonHistoryRole> {
    items.iter().find_map(common_history_role)
}

pub(super) fn next_common_item_is_tool_call(items: &[ResponseItem]) -> bool {
    items
        .iter()
        .find_map(|item| {
            if common_reasoning_content(item).is_some() {
                None
            } else {
                Some(response_item_is_common_tool_call(item))
            }
        })
        .unwrap_or(false)
}

pub(super) fn common_reasoning_content(item: &ResponseItem) -> Option<String> {
    let ResponseItem::Reasoning { content, .. } = item else {
        return None;
    };
    let text = content
        .as_ref()?
        .iter()
        .map(|part| match part {
            ReasoningItemContent::ReasoningText { text } | ReasoningItemContent::Text { text } => {
                text.as_str()
            }
        })
        .collect::<String>();
    (!text.trim().is_empty()).then_some(text)
}

pub(super) fn append_common_reasoning_content(target: &mut String, content: &str) {
    if content.trim().is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(content);
}

pub(super) fn attach_common_reasoning_content(
    message: &mut Value,
    pending_reasoning_content: &mut String,
    compat: &CommonRequestCompat,
) {
    if pending_reasoning_content.trim().is_empty() {
        pending_reasoning_content.clear();
        return;
    }
    let Some(replay_field) = CommonThinkingPolicy::from_format(compat.thinking_format).replay_field
    else {
        pending_reasoning_content.clear();
        return;
    };
    if let Value::Object(map) = message {
        map.insert(
            replay_field.to_string(),
            Value::String(std::mem::take(pending_reasoning_content)),
        );
    } else {
        pending_reasoning_content.clear();
    }
}

pub(super) fn ensure_common_assistant_message(
    pending_assistant_message: &mut Option<Value>,
) -> &mut Value {
    if pending_assistant_message.is_none() {
        *pending_assistant_message = Some(json!({
            "role": "assistant",
            "content": "",
        }));
    }
    pending_assistant_message
        .as_mut()
        .expect("pending assistant message should be initialized")
}

pub(super) fn append_common_tool_call(message: &mut Value, tool_call: Value) {
    let Value::Object(map) = message else {
        return;
    };
    match map.get_mut("tool_calls") {
        Some(Value::Array(tool_calls)) => tool_calls.push(tool_call),
        _ => {
            map.insert("tool_calls".to_string(), Value::Array(vec![tool_call]));
        }
    }
}

pub(super) fn extract_common_tool_call_provider_metadata(tool_call: &Value) -> Option<Value> {
    let Value::Object(root) = tool_call else {
        return None;
    };
    let mut metadata = serde_json::Map::new();
    for (key, value) in root {
        if !matches!(key.as_str(), "id" | "type" | "function" | "index") {
            metadata.insert(key.clone(), value.clone());
        }
    }
    if let Some(Value::Object(function)) = root.get("function") {
        let mut function_metadata = serde_json::Map::new();
        for (key, value) in function {
            if !matches!(key.as_str(), "name" | "arguments") {
                function_metadata.insert(key.clone(), value.clone());
            }
        }
        if !function_metadata.is_empty() {
            metadata.insert("function".to_string(), Value::Object(function_metadata));
        }
    }
    (!metadata.is_empty()).then_some(Value::Object(metadata))
}

pub(super) fn merge_common_tool_call_provider_metadata(
    tool_call: &mut Value,
    provider_metadata: Option<&Value>,
) {
    let Some(Value::Object(metadata)) = provider_metadata else {
        return;
    };
    let Value::Object(tool_call_map) = tool_call else {
        return;
    };
    for (key, value) in metadata {
        if key == "function" {
            let Value::Object(function_metadata) = value else {
                continue;
            };
            if let Some(Value::Object(function_map)) = tool_call_map.get_mut("function") {
                for (function_key, function_value) in function_metadata {
                    function_map.insert(function_key.clone(), function_value.clone());
                }
            }
        } else {
            tool_call_map.insert(key.clone(), value.clone());
        }
    }
}

pub(super) fn merge_common_tool_call_state_provider_metadata(
    tool_call: &mut CommonToolCallState,
    metadata: Option<Value>,
) {
    let Some(metadata) = metadata else {
        return;
    };
    match (&mut tool_call.provider_metadata, metadata) {
        (Some(Value::Object(existing)), Value::Object(update)) => {
            for (key, value) in update {
                existing.insert(key, value);
            }
        }
        (slot, metadata) => {
            *slot = Some(metadata);
        }
    }
}

pub(super) fn flush_common_assistant_message(
    messages: &mut Vec<Value>,
    pending_assistant_message: &mut Option<Value>,
) {
    if let Some(message) = pending_assistant_message.take() {
        messages.push(message);
    }
}

pub(super) fn collect_system_prompt(prompt: &Prompt, items: &[ResponseItem]) -> String {
    let mut sections = Vec::new();

    if !prompt.base_instructions.text.trim().is_empty() {
        sections.push(prompt.base_instructions.text.trim().to_string());
    }

    sections.extend(items.iter().filter_map(|item| match item {
        ResponseItem::Message { role, content, .. } if role == "system" || role == "developer" => {
            let rendered = render_text_only_content(content);
            (!rendered.trim().is_empty()).then_some(rendered)
        }
        _ => None,
    }));

    sections.join("\n\n")
}

use super::*;

pub(super) fn parse_claude_response(response_json: Value) -> Result<ParsedProviderResponse> {
    let response_id = response_json
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("claude-{}", Uuid::new_v4()));

    let content = response_json
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PraxisErr::InvalidRequest(
                "provider returned invalid claude response: missing `content` array".to_string(),
            )
        })?;

    let mut items = Vec::new();
    let mut text_parts = Vec::new();

    for part in content {
        match part.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = part.get("text").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    text_parts.push(text.to_string());
                }
            }
            Some("tool_use") => {
                let name = part
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        PraxisErr::InvalidRequest(
                            "provider returned invalid claude response: tool_use missing `name`"
                                .to_string(),
                        )
                    })?
                    .to_string();
                let call_id = part
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("claude-tool-{}", Uuid::new_v4()));
                let input = part.get("input").cloned().unwrap_or_else(|| json!({}));
                items.push(ResponseItem::FunctionCall {
                    id: None,
                    provider_metadata: None,
                    name,
                    namespace: None,
                    arguments: serde_json::to_string(&input)?,
                    call_id,
                });
            }
            _ => {}
        }
    }

    if !text_parts.is_empty() {
        items.insert(
            0,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: text_parts.join(""),
                }],
                end_turn: None,
                phase: None,
            },
        );
    }

    Ok(ParsedProviderResponse {
        response_id,
        token_usage: parse_claude_usage(response_json.get("usage")),
        items,
    })
}

pub(super) fn parse_common_response(
    response_json: Value,
    thinking_policy: CommonThinkingPolicy,
) -> Result<ParsedProviderResponse> {
    let response_id = response_json
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("common-{}", Uuid::new_v4()));

    let message = response_json
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| {
            PraxisErr::InvalidRequest(
                "provider returned invalid common response: missing `choices[0].message`"
                    .to_string(),
            )
        })?;

    let mut items = Vec::new();

    if let Some(reasoning_content) = extract_common_reasoning_content(message, thinking_policy) {
        items.push(common_reasoning_item(reasoning_content));
    }

    if let Some(text) = extract_common_response_text(message.get("content")) {
        for segment in split_common_think_tag_segments(&text) {
            match segment {
                CommonThinkSegment::Text(text) => push_common_message_item(&mut items, text),
                CommonThinkSegment::Reasoning(text) => items.push(common_reasoning_item(text)),
            }
        }
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            let function = tool_call.get("function").ok_or_else(|| {
                PraxisErr::InvalidRequest(
                    "provider returned invalid common response: tool call missing `function`"
                        .to_string(),
                )
            })?;
            let raw_name = function
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string);
            let arguments = function
                .get("arguments")
                .map(value_to_json_string)
                .unwrap_or_else(|| "{}".to_string());
            let name = normalize_common_tool_call_name(raw_name, &arguments).ok_or_else(|| {
                PraxisErr::InvalidRequest(
                    "provider returned invalid common response: tool function missing `name`"
                        .to_string(),
                )
            })?;
            let call_id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|call_id| !call_id.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("common-tool-{index}-{}", Uuid::new_v4()));
            items.push(ResponseItem::FunctionCall {
                id: None,
                provider_metadata: extract_common_tool_call_provider_metadata(tool_call),
                name,
                namespace: None,
                arguments,
                call_id,
            });
        }
    }

    Ok(ParsedProviderResponse {
        response_id,
        token_usage: parse_common_usage(response_json.get("usage")),
        items,
    })
}

pub(super) fn normalize_common_tool_call_name(
    name: Option<String>,
    arguments: &str,
) -> Option<String> {
    if let Some(name) = name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return Some(name.to_string());
    }

    infer_common_tool_call_name_from_arguments(arguments).map(str::to_string)
}

pub(super) fn infer_common_tool_call_name_from_arguments(arguments: &str) -> Option<&'static str> {
    let value = serde_json::from_str::<Value>(arguments).ok()?;
    let map = value.as_object()?;

    if map.contains_key("task_name") && map.contains_key("message") {
        return Some("spawn_agent");
    }
    if map.contains_key("timeout_ms") && map.len() == 1 {
        return Some("wait_agent");
    }
    if map.contains_key("path_prefix") {
        return Some("list_agents");
    }
    if map.contains_key("target") && map.contains_key("message") {
        return Some("send_message");
    }
    if map.contains_key("target") && map.contains_key("objective") {
        return Some("assign_task");
    }
    None
}

pub(super) fn extract_common_reasoning_content(
    message: &Value,
    thinking_policy: CommonThinkingPolicy,
) -> Option<String> {
    thinking_policy
        .response_fields
        .iter()
        .find_map(|key| message.get(key).and_then(Value::as_str))
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

pub(super) fn extract_common_reasoning_delta(
    delta: &Value,
    thinking_policy: CommonThinkingPolicy,
) -> Option<String> {
    thinking_policy
        .response_fields
        .iter()
        .find_map(|key| extract_common_stream_delta_text(delta.get(key)))
}

pub(super) fn common_reasoning_item(text: String) -> ResponseItem {
    common_reasoning_item_with_id(format!("common-reasoning-{}", Uuid::new_v4()), text)
}

pub(super) fn common_reasoning_item_with_id(id: String, text: String) -> ResponseItem {
    ResponseItem::Reasoning {
        id,
        summary: Vec::new(),
        content: Some(vec![ReasoningItemContent::ReasoningText { text }]),
        encrypted_content: None,
    }
}

pub(super) fn push_common_message_item(items: &mut Vec<ResponseItem>, text: String) {
    if text.is_empty() {
        return;
    }
    items.push(ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        end_turn: None,
        phase: None,
    });
}

pub(super) fn split_common_think_tag_segments(text: &str) -> Vec<CommonThinkSegment> {
    let mut parser = CommonThinkTagStreamState::default();
    parser.pending.push_str(text);
    parser.finish()
}

pub(super) fn extract_common_response_text(content: Option<&Value>) -> Option<String> {
    let content = content?;
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                    Some("text") | Some("output_text") => {
                        part.get("text").and_then(Value::as_str).map(str::to_string)
                    }
                    _ => None,
                })
                .collect::<String>(),
        ),
        Value::Null => None,
        _ => Some(content.to_string()),
    }
}

pub(super) fn parse_claude_usage(usage: Option<&Value>) -> Option<TokenUsage> {
    let usage = usage?;
    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let cached_input_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let cache_reported_input_tokens = if usage.get("cache_read_input_tokens").is_some()
        || usage.get("cache_creation_input_tokens").is_some()
    {
        input_tokens.max(0)
    } else {
        0
    };
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens);

    Some(TokenUsage {
        input_tokens,
        cached_input_tokens,
        cache_reported_input_tokens,
        output_tokens,
        reasoning_output_tokens: 0,
        total_tokens,
    })
}

pub(super) fn parse_common_usage(usage: Option<&Value>) -> Option<TokenUsage> {
    let usage = usage?;
    let prompt_cache_hit_tokens = usage
        .get("prompt_cache_hit_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    let prompt_cache_miss_tokens = usage
        .get("prompt_cache_miss_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    let cache_split_input_tokens = prompt_cache_hit_tokens + prompt_cache_miss_tokens;
    let has_deepseek_cache_split = usage.get("prompt_cache_hit_tokens").is_some()
        || usage.get("prompt_cache_miss_tokens").is_some();
    let has_openai_cache_details = usage
        .get("prompt_tokens_details")
        .and_then(|details| details.get("cached_tokens"))
        .is_some();
    let input_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage.get("input_tokens").and_then(Value::as_i64))
        .unwrap_or(cache_split_input_tokens)
        .max(0)
        .max(cache_split_input_tokens);
    let output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage.get("output_tokens").and_then(Value::as_i64))
        .unwrap_or(0)
        .max(0);
    let openai_cached_input_tokens = usage
        .get("prompt_tokens_details")
        .and_then(|details| details.get("cached_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    let cached_input_tokens = openai_cached_input_tokens
        .max(prompt_cache_hit_tokens)
        .min(input_tokens);
    let cache_reported_input_tokens = if has_deepseek_cache_split {
        cache_split_input_tokens.max(input_tokens)
    } else if has_openai_cache_details {
        input_tokens
    } else {
        0
    };
    let reasoning_output_tokens = usage
        .get("completion_tokens_details")
        .and_then(|details| details.get("reasoning_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens)
        .max(0);

    Some(TokenUsage {
        input_tokens,
        cached_input_tokens,
        cache_reported_input_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
}

pub(super) fn value_to_json_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub(super) fn build_response_stream(parsed: ParsedProviderResponse) -> Result<ResponseStream> {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(64);
    let ParsedProviderResponse {
        response_id,
        token_usage,
        items,
    } = parsed;

    tx_event
        .try_send(Ok(ResponseEvent::Created))
        .map_err(|err| {
            PraxisErr::Fatal(format!("failed to emit provider response start: {err}"))
        })?;
    for item in items {
        tx_event
            .try_send(Ok(ResponseEvent::OutputItemDone(item)))
            .map_err(|err| {
                PraxisErr::Fatal(format!("failed to emit provider response item: {err}"))
            })?;
    }
    tx_event
        .try_send(Ok(ResponseEvent::Completed {
            response_id,
            token_usage,
        }))
        .map_err(|err| PraxisErr::Fatal(format!("failed to emit provider response end: {err}")))?;

    Ok(ResponseStream { rx_event })
}

pub(super) fn map_reqwest_error(err: reqwest::Error) -> PraxisErr {
    if err.is_timeout() {
        return map_api_error(ApiError::Transport(TransportError::Timeout));
    }
    if err.is_connect() || err.is_request() || err.is_body() {
        return map_api_error(ApiError::Transport(TransportError::Network(
            err.to_string(),
        )));
    }
    PraxisErr::ConnectionFailed(crate::error::ConnectionFailedError { source: err })
}

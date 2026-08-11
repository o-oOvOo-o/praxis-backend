use super::super::*;
use super::send_stream_event;

#[derive(Default)]
pub(in super::super) struct ClaudeStreamState {
    response_id: Option<String>,
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_creation_input_tokens: i64,
    cache_reported_input_tokens: i64,
    cache_accounting_reported: bool,
    output_tokens: i64,
    message_text: String,
    message_open: bool,
    thinking_blocks: BTreeMap<i64, ClaudeThinkingBlockState>,
    tool_blocks: BTreeMap<i64, ClaudeToolBlockState>,
}

#[derive(Default)]
pub(in super::super) struct ClaudeThinkingBlockState {
    reasoning_id: Option<String>,
    block: Option<Value>,
    thinking: String,
    signature: String,
    item_added: bool,
}

#[derive(Default)]
pub(in super::super) struct ClaudeToolBlockState {
    call_id: Option<String>,
    name: Option<String>,
    initial_input: Option<Value>,
    partial_json: String,
}

pub(in super::super) async fn process_claude_sse(
    response: reqwest::Response,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
    idle_timeout: Duration,
) {
    if tx_event.send(Ok(ResponseEvent::Created)).await.is_err() {
        return;
    }

    let mut stream = response.bytes_stream().eventsource();
    let mut state = ClaudeStreamState::default();

    loop {
        let next = timeout(idle_timeout, stream.next()).await;
        let sse = match next {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(err))) => {
                let _ = err;
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        "claude stream transport error".to_string(),
                        None,
                    )))
                    .await;
                return;
            }
            Ok(None) => {
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        "claude stream closed before message_stop".to_string(),
                        None,
                    )))
                    .await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        "idle timeout waiting for claude stream".to_string(),
                        None,
                    )))
                    .await;
                return;
            }
        };

        let event: Value = match serde_json::from_str(&sse.data) {
            Ok(event) => event,
            Err(_) if !sse.event.is_empty() && !is_known_claude_sse_event_name(&sse.event) => {
                continue;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        "invalid claude stream event".to_string(),
                        None,
                    )))
                    .await;
                return;
            }
        };

        match process_claude_stream_event(&mut state, &tx_event, event).await {
            Ok(done) => {
                if done {
                    return;
                }
            }
            Err(err) => {
                let _ = tx_event.send(Err(err)).await;
                return;
            }
        }
    }
}

pub(in super::super) fn is_known_claude_sse_event_name(event: &str) -> bool {
    matches!(
        event,
        "message_start"
            | "content_block_start"
            | "content_block_delta"
            | "content_block_stop"
            | "message_delta"
            | "message_stop"
            | "error"
            | "ping"
    )
}

pub(in super::super) async fn process_claude_stream_event(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    event: Value,
) -> Result<bool> {
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match event_type {
        "message_start" => {
            let message = event.get("message").ok_or_else(|| {
                PraxisErr::InvalidRequest(
                    "provider returned invalid claude stream: message_start missing `message`"
                        .to_string(),
                )
            })?;
            let response_id = message
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|response_id| !response_id.is_empty())
                .ok_or_else(|| {
                    PraxisErr::InvalidRequest(
                        "provider returned invalid claude stream: message_start missing `message.id`"
                            .to_string(),
                    )
                })?;
            state.response_id = Some(response_id.to_string());
            update_claude_usage(state, message.get("usage"));
        }
        "content_block_start" => {
            let index = event.get("index").and_then(Value::as_i64).unwrap_or(0);
            let Some(block) = event.get("content_block") else {
                return Ok(false);
            };
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    emit_claude_reasoning_before(state, tx_event, index).await?;
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        emit_claude_text_delta(state, tx_event, text).await?;
                    }
                }
                Some("thinking") => {
                    emit_claude_message_done(state, tx_event).await?;
                    let initial_thinking = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let entry = state.thinking_blocks.entry(index).or_default();
                    entry.block = Some(block.clone());
                    entry.signature = block
                        .get("signature")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if !initial_thinking.is_empty() {
                        emit_claude_thinking_delta(state, tx_event, index, &initial_thinking)
                            .await?;
                    }
                }
                Some("redacted_thinking") => {
                    emit_claude_message_done(state, tx_event).await?;
                    state.thinking_blocks.entry(index).or_default().block = Some(block.clone());
                }
                Some("tool_use") => {
                    emit_claude_reasoning_before(state, tx_event, index).await?;
                    emit_claude_message_done(state, tx_event).await?;
                    let entry = state.tool_blocks.entry(index).or_default();
                    entry.call_id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|call_id| !call_id.is_empty())
                        .map(str::to_string);
                    entry.name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if let Some(input) = block.get("input")
                        && !value_is_empty_object(input)
                    {
                        entry.initial_input = Some(input.clone());
                    }
                }
                _ => {}
            }
        }
        "content_block_delta" => {
            let index = event.get("index").and_then(Value::as_i64).unwrap_or(0);
            let Some(delta) = event.get("delta") else {
                return Ok(false);
            };
            match delta.get("type").and_then(Value::as_str) {
                Some("text_delta") => {
                    emit_claude_reasoning_before(state, tx_event, index).await?;
                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                        emit_claude_text_delta(state, tx_event, text).await?;
                    }
                }
                Some("thinking_delta") => {
                    let thinking = delta
                        .get("thinking")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    emit_claude_thinking_delta(state, tx_event, index, thinking).await?;
                }
                Some("signature_delta") => {
                    if let Some(signature) = delta.get("signature").and_then(Value::as_str) {
                        state
                            .thinking_blocks
                            .entry(index)
                            .or_default()
                            .signature
                            .push_str(signature);
                    }
                }
                Some("input_json_delta") => {
                    emit_claude_reasoning_before(state, tx_event, index).await?;
                    emit_claude_message_done(state, tx_event).await?;
                    let partial_json = delta
                        .get("partial_json")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    state
                        .tool_blocks
                        .entry(index)
                        .or_default()
                        .partial_json
                        .push_str(partial_json);
                }
                _ => {}
            }
        }
        "content_block_stop" => {
            let index = event.get("index").and_then(Value::as_i64).unwrap_or(0);
            emit_claude_reasoning_done(state, tx_event, index).await?;
            emit_claude_tool_done(state, tx_event, index).await?;
        }
        "message_delta" => {
            update_claude_usage(state, event.get("usage"));
        }
        "message_stop" => {
            emit_all_claude_reasoning_done(state, tx_event).await?;
            emit_claude_message_done(state, tx_event).await?;
            let tool_indexes = state.tool_blocks.keys().copied().collect::<Vec<_>>();
            for index in tool_indexes {
                emit_claude_tool_done(state, tx_event, index).await?;
            }
            let response_id = state.response_id.clone().ok_or_else(|| {
                PraxisErr::InvalidRequest(
                    "provider returned invalid claude stream: message_stop before message_start"
                        .to_string(),
                )
            })?;
            let token_usage = Some(TokenUsage {
                input_tokens: state
                    .input_tokens
                    .saturating_add(state.cached_input_tokens)
                    .saturating_add(state.cache_creation_input_tokens),
                cached_input_tokens: state.cached_input_tokens,
                cache_reported_input_tokens: state.cache_reported_input_tokens,
                output_tokens: state.output_tokens,
                reasoning_output_tokens: 0,
                total_tokens: state
                    .input_tokens
                    .saturating_add(state.cached_input_tokens)
                    .saturating_add(state.cache_creation_input_tokens)
                    .saturating_add(state.output_tokens),
            });
            send_stream_event(
                tx_event,
                ResponseEvent::Completed {
                    response_id,
                    token_usage,
                },
            )
            .await?;
            return Ok(true);
        }
        "error" => {
            let error_type = event
                .get("error")
                .and_then(|error| error.get("type"))
                .and_then(Value::as_str)
                .filter(|error_type| {
                    !error_type.is_empty()
                        && error_type.len() <= 64
                        && error_type
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                });
            let message = error_type
                .map(|error_type| format!("claude stream error ({error_type})"))
                .unwrap_or_else(|| "claude stream error".to_string());
            return Err(PraxisErr::Stream(message, None));
        }
        "ping" => {}
        _ => {}
    }

    Ok(false)
}

pub(in super::super) async fn emit_claude_thinking_delta(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    index: i64,
    delta: &str,
) -> Result<()> {
    if delta.is_empty() {
        return Ok(());
    }

    let (reasoning_id, should_add) = {
        let entry = state.thinking_blocks.entry(index).or_default();
        let reasoning_id = entry
            .reasoning_id
            .get_or_insert_with(|| format!("claude-reasoning-{index}-{}", Uuid::new_v4()))
            .clone();
        let should_add = !entry.item_added;
        entry.item_added = true;
        entry.thinking.push_str(delta);
        (reasoning_id, should_add)
    };

    if should_add {
        send_stream_event(
            tx_event,
            ResponseEvent::OutputItemAdded(ResponseItem::Reasoning {
                id: reasoning_id,
                summary: Vec::new(),
                content: Some(Vec::new()),
                encrypted_content: None,
            }),
        )
        .await?;
    }
    send_stream_event(
        tx_event,
        ResponseEvent::ReasoningContentDelta {
            delta: delta.to_string(),
            content_index: 0,
        },
    )
    .await
}

pub(in super::super) async fn emit_claude_reasoning_done(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    index: i64,
) -> Result<()> {
    let Some(reasoning) = state.thinking_blocks.remove(&index) else {
        return Ok(());
    };
    let mut block = reasoning
        .block
        .unwrap_or_else(|| json!({ "type": "thinking" }));
    if block.get("type").and_then(Value::as_str) == Some("thinking") {
        let object = block.as_object_mut().ok_or_else(|| {
            PraxisErr::InvalidRequest("invalid Claude thinking stream block".to_string())
        })?;
        object.insert("thinking".to_string(), Value::String(reasoning.thinking));
        object.insert("signature".to_string(), Value::String(reasoning.signature));
    }
    let id = reasoning
        .reasoning_id
        .unwrap_or_else(|| format!("claude-reasoning-{index}-{}", Uuid::new_v4()));
    send_stream_event(
        tx_event,
        ResponseEvent::OutputItemDone(claude_reasoning_item(block, id)?),
    )
    .await
}

pub(in super::super) async fn emit_claude_reasoning_before(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    index: i64,
) -> Result<()> {
    let indexes = state
        .thinking_blocks
        .range(..index)
        .map(|(index, _)| *index)
        .collect::<Vec<_>>();
    for index in indexes {
        emit_claude_reasoning_done(state, tx_event, index).await?;
    }
    Ok(())
}

pub(in super::super) async fn emit_all_claude_reasoning_done(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    let indexes = state.thinking_blocks.keys().copied().collect::<Vec<_>>();
    for index in indexes {
        emit_claude_reasoning_done(state, tx_event, index).await?;
    }
    Ok(())
}

pub(in super::super) async fn emit_claude_text_delta(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    delta: &str,
) -> Result<()> {
    if delta.is_empty() {
        return Ok(());
    }
    if !state.message_open {
        send_stream_event(
            tx_event,
            ResponseEvent::OutputItemAdded(ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: String::new(),
                }],
                end_turn: None,
                phase: None,
            }),
        )
        .await?;
        state.message_open = true;
    }
    state.message_text.push_str(delta);
    send_stream_event(tx_event, ResponseEvent::OutputTextDelta(delta.to_string())).await
}

pub(in super::super) async fn emit_claude_message_done(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    if !state.message_open {
        return Ok(());
    }
    let text = std::mem::take(&mut state.message_text);
    state.message_open = false;
    send_stream_event(
        tx_event,
        ResponseEvent::OutputItemDone(ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText { text }],
            end_turn: None,
            phase: None,
        }),
    )
    .await
}

pub(in super::super) async fn emit_claude_tool_done(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    index: i64,
) -> Result<()> {
    let Some(tool) = state.tool_blocks.remove(&index) else {
        return Ok(());
    };
    let name = tool.name.ok_or_else(|| {
        PraxisErr::InvalidRequest(
            "provider returned invalid claude stream: tool_use missing `name`".to_string(),
        )
    })?;
    validate_claude_tool_name(&name).map_err(|_| {
        PraxisErr::InvalidRequest(
            "provider returned invalid claude stream: tool_use has an invalid `name`".to_string(),
        )
    })?;
    let call_id = tool.call_id.ok_or_else(|| {
        PraxisErr::InvalidRequest(
            "provider returned invalid claude stream: tool_use missing `id`".to_string(),
        )
    })?;
    let input = finalize_claude_tool_input(tool.initial_input, &tool.partial_json)?;
    send_stream_event(
        tx_event,
        ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name,
            namespace: None,
            arguments: serde_json::to_string(&input)?,
            call_id,
        }),
    )
    .await
}

pub(in super::super) fn finalize_claude_tool_input(
    initial_input: Option<Value>,
    partial_json: &str,
) -> Result<Value> {
    if !partial_json.is_empty() {
        let value = serde_json::from_str::<Value>(partial_json).map_err(|_| {
            PraxisErr::InvalidRequest(
                "provider returned invalid claude stream: tool_use input is not valid JSON"
                    .to_string(),
            )
        })?;
        if !value.is_object() {
            return Err(PraxisErr::InvalidRequest(
                "provider returned invalid claude stream: tool_use input must be an object"
                    .to_string(),
            ));
        }
        return Ok(value);
    }

    let input = initial_input.unwrap_or_else(|| json!({}));
    if !input.is_object() {
        return Err(PraxisErr::InvalidRequest(
            "provider returned invalid claude stream: tool_use input must be an object".to_string(),
        ));
    }
    Ok(input)
}

pub(in super::super) fn update_claude_usage(state: &mut ClaudeStreamState, usage: Option<&Value>) {
    let Some(usage) = usage else {
        return;
    };
    if let Some(input_tokens) = usage.get("input_tokens").and_then(Value::as_i64) {
        state.input_tokens = input_tokens;
    }
    if let Some(cached_input_tokens) = usage.get("cache_read_input_tokens").and_then(Value::as_i64)
    {
        state.cached_input_tokens = cached_input_tokens.max(0);
    }
    if let Some(cache_creation_input_tokens) = usage
        .get("cache_creation_input_tokens")
        .and_then(Value::as_i64)
    {
        state.cache_creation_input_tokens = cache_creation_input_tokens.max(0);
    }
    if usage.get("cache_read_input_tokens").is_some()
        || usage.get("cache_creation_input_tokens").is_some()
    {
        state.cache_accounting_reported = true;
        state.cache_reported_input_tokens = state
            .input_tokens
            .max(0)
            .saturating_add(state.cached_input_tokens)
            .saturating_add(state.cache_creation_input_tokens);
    }
    if state.cache_accounting_reported {
        state.cache_reported_input_tokens = state
            .input_tokens
            .max(0)
            .saturating_add(state.cached_input_tokens)
            .saturating_add(state.cache_creation_input_tokens);
    }
    if let Some(output_tokens) = usage.get("output_tokens").and_then(Value::as_i64) {
        state.output_tokens = output_tokens;
    }
}

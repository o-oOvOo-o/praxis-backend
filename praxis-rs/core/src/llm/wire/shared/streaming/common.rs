use super::super::*;
use super::send_stream_event;
use super::think_tags::*;

#[derive(Default)]
pub(in super::super) struct CommonStreamState {
    response_id: Option<String>,
    reasoning_text: String,
    reasoning_open: bool,
    reasoning_id: Option<String>,
    message_text: String,
    message_open: bool,
    think_tag_parser: CommonThinkTagStreamState,
    tool_calls: BTreeMap<usize, CommonToolCallState>,
    tool_calls_emitted: bool,
    token_usage: Option<TokenUsage>,
    saw_finish_reason: bool,
    finish_reason_at: Option<Instant>,
    last_content_delta_at: Option<Instant>,
}

#[derive(Default)]
pub(in super::super) struct CommonToolCallState {
    pub(in super::super) call_id: Option<String>,
    pub(in super::super) name: Option<String>,
    pub(in super::super) arguments: String,
    pub(in super::super) provider_metadata: Option<Value>,
}

pub(in super::super) async fn process_common_sse(
    response: reqwest::Response,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
    idle_timeout: Duration,
    thinking_policy: CommonThinkingPolicy,
) {
    if tx_event.send(Ok(ResponseEvent::Created)).await.is_err() {
        return;
    }

    let mut stream = response.bytes_stream().eventsource();
    let mut state = CommonStreamState::default();

    loop {
        if common_should_complete_now(&state, thinking_policy) {
            match emit_common_completion(&mut state, &tx_event).await {
                Ok(()) => return,
                Err(err) => {
                    let _ = tx_event.send(Err(err)).await;
                    return;
                }
            }
        }

        let wait_timeout = common_next_wait_timeout(&state, thinking_policy, idle_timeout);
        let next = timeout(wait_timeout, stream.next()).await;
        let sse = match next {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(err))) => {
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        format!("common stream error: {err}"),
                        None,
                    )))
                    .await;
                return;
            }
            Ok(None) => {
                if common_can_complete_on_stream_close(&state, thinking_policy) {
                    tracing::warn!(
                        "common stream closed before [DONE]; completing from buffered output"
                    );
                    match emit_common_completion(&mut state, &tx_event).await {
                        Ok(()) => return,
                        Err(err) => {
                            let _ = tx_event.send(Err(err)).await;
                            return;
                        }
                    }
                }
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        "common stream closed before [DONE]".to_string(),
                        None,
                    )))
                    .await;
                return;
            }
            Err(_) => {
                if common_can_complete_on_timeout(&state, thinking_policy) {
                    match emit_common_completion(&mut state, &tx_event).await {
                        Ok(()) => return,
                        Err(err) => {
                            let _ = tx_event.send(Err(err)).await;
                            return;
                        }
                    }
                }
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        "idle timeout waiting for common stream".to_string(),
                        None,
                    )))
                    .await;
                return;
            }
        };

        match process_common_stream_event(&mut state, &tx_event, &sse.data, thinking_policy).await {
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

pub(in super::super) async fn process_common_stream_event(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    payload: &str,
    thinking_policy: CommonThinkingPolicy,
) -> Result<bool> {
    if payload.trim() == "[DONE]" {
        emit_common_completion(state, tx_event).await?;
        return Ok(true);
    }

    let chunk: Value = serde_json::from_str(payload)?;
    if let Some(response_id) = chunk.get("id").and_then(Value::as_str) {
        state.response_id = Some(response_id.to_string());
    }
    if let Some(usage) = parse_common_usage(common_usage_value(&chunk)) {
        state.token_usage = Some(usage);
    }

    let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
        return Ok(false);
    };

    let mut should_complete_after_finish = false;
    for choice in choices {
        let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
        if let Some(delta) = choice.get("delta") {
            if let Some(reasoning) = extract_common_reasoning_delta(delta, thinking_policy)
                && !reasoning.is_empty()
            {
                emit_common_reasoning_delta(state, tx_event, &reasoning).await?;
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                emit_common_content_done(state, tx_event).await?;
                emit_common_reasoning_done(state, tx_event).await?;
                if state.message_open {
                    emit_common_message_done(state, tx_event).await?;
                }
                for (fallback_index, tool_call) in tool_calls.iter().enumerate() {
                    let index = tool_call
                        .get("index")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize)
                        .unwrap_or(fallback_index);
                    let entry = state.tool_calls.entry(index).or_default();
                    if let Some(call_id) = tool_call
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|call_id| !call_id.is_empty())
                    {
                        entry.call_id = Some(call_id.to_string());
                    }
                    if let Some(name) = tool_call
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                    {
                        entry.name = Some(name.to_string());
                    }
                    if let Some(arguments) = tool_call
                        .get("function")
                        .and_then(|function| function.get("arguments"))
                        .and_then(Value::as_str)
                    {
                        entry.arguments.push_str(arguments);
                    }
                    merge_common_tool_call_state_provider_metadata(
                        entry,
                        extract_common_tool_call_provider_metadata(tool_call),
                    );
                }
            }

            if let Some(text) = extract_common_stream_delta_text(delta.get("content"))
                && !text.is_empty()
            {
                emit_common_content_delta(state, tx_event, &text).await?;
            }
        }

        if let Some(reason) = finish_reason {
            state.saw_finish_reason = true;
            state.finish_reason_at.get_or_insert_with(Instant::now);
            match reason {
                "tool_calls" => {
                    emit_common_content_done(state, tx_event).await?;
                    emit_common_message_done(state, tx_event).await?;
                    emit_common_tool_calls(state, tx_event).await?;
                }
                "stop" | "length" | "content_filter" => {
                    emit_common_content_done(state, tx_event).await?;
                    emit_common_message_done(state, tx_event).await?;
                }
                _ => {}
            }
            should_complete_after_finish |= thinking_policy.complete_on_finish_reason;
        }
    }

    if should_complete_after_finish {
        emit_common_completion(state, tx_event).await?;
        return Ok(true);
    }

    Ok(false)
}

pub(in super::super) async fn emit_common_text_delta(
    state: &mut CommonStreamState,
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
    state.last_content_delta_at = Some(Instant::now());
    send_stream_event(tx_event, ResponseEvent::OutputTextDelta(delta.to_string())).await
}

pub(in super::super) async fn emit_common_content_delta(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    delta: &str,
) -> Result<()> {
    let segments = state.think_tag_parser.push(delta);
    emit_common_content_segments(state, tx_event, segments).await
}

pub(in super::super) async fn emit_common_content_done(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    let segments = state.think_tag_parser.finish();
    emit_common_content_segments(state, tx_event, segments).await
}

pub(in super::super) async fn emit_common_content_segments(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    segments: Vec<CommonThinkSegment>,
) -> Result<()> {
    for segment in segments {
        match segment {
            CommonThinkSegment::Text(text) => {
                emit_common_reasoning_done(state, tx_event).await?;
                emit_common_text_delta(state, tx_event, &text).await?;
            }
            CommonThinkSegment::Reasoning(text) => {
                emit_common_message_done(state, tx_event).await?;
                emit_common_reasoning_delta(state, tx_event, &text).await?;
            }
        }
    }
    Ok(())
}

pub(in super::super) async fn emit_common_reasoning_delta(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    delta: &str,
) -> Result<()> {
    if delta.is_empty() {
        return Ok(());
    }
    if !state.reasoning_open {
        let id = state
            .reasoning_id
            .get_or_insert_with(|| format!("common-reasoning-{}", Uuid::new_v4()))
            .clone();
        send_stream_event(
            tx_event,
            ResponseEvent::OutputItemAdded(common_reasoning_item_with_id(id, String::new())),
        )
        .await?;
        state.reasoning_open = true;
    }
    state.reasoning_text.push_str(delta);
    state.last_content_delta_at = Some(Instant::now());
    send_stream_event(
        tx_event,
        ResponseEvent::ReasoningContentDelta {
            delta: delta.to_string(),
            content_index: 0,
        },
    )
    .await
}

pub(in super::super) fn common_next_wait_timeout(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
    idle_timeout: Duration,
) -> Duration {
    let Some(deadline) = common_completion_deadline(state, thinking_policy) else {
        return idle_timeout;
    };
    deadline
        .saturating_duration_since(Instant::now())
        .min(idle_timeout)
}

pub(in super::super) fn common_should_complete_now(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    common_completion_deadline(state, thinking_policy)
        .is_some_and(|deadline| Instant::now() >= deadline)
}

/// Whether this stream attempt has produced any real output so far.
///
/// A dying proxy connection can deliver a bare `finish_reason` chunk and then
/// close without content and without `[DONE]`. Treating that as a successful
/// completion produces a silent empty turn downstream, so completion-on-close
/// and completion-on-timeout both require actual output.
pub(in super::super) fn common_stream_produced_output(state: &CommonStreamState) -> bool {
    !state.message_text.is_empty()
        || !state.reasoning_text.is_empty()
        || !state.tool_calls.is_empty()
        || state.tool_calls_emitted
}

pub(in super::super) fn common_can_complete_on_timeout(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    (state.saw_finish_reason && common_stream_produced_output(state))
        || common_can_complete_on_message_idle(state, thinking_policy)
}

pub(in super::super) fn common_can_complete_on_stream_close(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    // An abrupt close (no `[DONE]`) only counts as completion when the server
    // both said it finished and actually produced output. The message-idle
    // tolerance is deliberately NOT honored here: a connection that dies
    // mid-message must surface as a stream error so the retry layer runs,
    // instead of presenting truncated text as a complete answer.
    let _ = thinking_policy;
    state.saw_finish_reason && common_stream_produced_output(state)
}

pub(in super::super) fn common_completion_deadline(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> Option<Instant> {
    let finish_deadline = state
        .finish_reason_at
        .map(|at| at + Duration::from_millis(COMMON_POST_FINISH_GRACE_MS));
    let message_idle_deadline = if common_can_complete_on_message_idle(state, thinking_policy) {
        state
            .last_content_delta_at
            .map(|at| at + Duration::from_millis(COMMON_DEEPSEEK_MESSAGE_IDLE_GRACE_MS))
    } else {
        None
    };

    match (finish_deadline, message_idle_deadline) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
        (None, None) => None,
    }
}

pub(in super::super) fn common_can_complete_on_message_idle(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    thinking_policy.complete_on_message_idle
        && state.message_open
        && !state.message_text.is_empty()
        && state.tool_calls.is_empty()
}

pub(in super::super) async fn emit_common_reasoning_done(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    let text = std::mem::take(&mut state.reasoning_text);
    if text.trim().is_empty() {
        state.reasoning_open = false;
        state.reasoning_id = None;
        return Ok(());
    }
    let id = state
        .reasoning_id
        .take()
        .unwrap_or_else(|| format!("common-reasoning-{}", Uuid::new_v4()));
    state.reasoning_open = false;
    send_stream_event(
        tx_event,
        ResponseEvent::OutputItemDone(common_reasoning_item_with_id(id, text)),
    )
    .await
}

pub(in super::super) async fn emit_common_message_done(
    state: &mut CommonStreamState,
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

pub(in super::super) async fn emit_common_tool_calls(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    if state.tool_calls_emitted {
        return Ok(());
    }
    let tool_calls = std::mem::take(&mut state.tool_calls);
    for (index, tool_call) in tool_calls {
        let arguments = if tool_call.arguments.is_empty() {
            "{}".to_string()
        } else {
            tool_call.arguments
        };
        let name = normalize_common_tool_call_name(tool_call.name, &arguments)
            .unwrap_or_else(|| format!("tool_{index}"));
        let call_id = tool_call
            .call_id
            .unwrap_or_else(|| format!("common-tool-{index}-{}", Uuid::new_v4()));
        send_stream_event(
            tx_event,
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
                id: None,
                provider_metadata: tool_call.provider_metadata,
                name,
                namespace: None,
                arguments,
                call_id,
            }),
        )
        .await?;
    }
    state.tool_calls_emitted = true;
    Ok(())
}

pub(in super::super) async fn emit_common_completion(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    emit_common_content_done(state, tx_event).await?;
    emit_common_reasoning_done(state, tx_event).await?;
    emit_common_message_done(state, tx_event).await?;
    emit_common_tool_calls(state, tx_event).await?;
    let response_id = state
        .response_id
        .clone()
        .unwrap_or_else(|| format!("common-{}", Uuid::new_v4()));
    send_stream_event(
        tx_event,
        ResponseEvent::Completed {
            response_id,
            token_usage: state.token_usage.take(),
        },
    )
    .await
}

pub(in super::super) fn extract_common_stream_delta_text(
    content: Option<&Value>,
) -> Option<String> {
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

pub(in super::super) fn value_is_empty_object(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.is_empty())
}

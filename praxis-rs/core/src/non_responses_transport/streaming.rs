use super::*;

pub(super) fn response_is_sse(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
}

pub(super) fn spawn_claude_sse_stream(
    response: reqwest::Response,
    idle_timeout: Duration,
) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(256);
    tokio::spawn(process_claude_sse(response, tx_event, idle_timeout));
    ResponseStream { rx_event }
}

pub(super) fn spawn_common_sse_stream(
    response: reqwest::Response,
    idle_timeout: Duration,
    thinking_policy: CommonThinkingPolicy,
) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(256);
    tokio::spawn(process_common_sse(
        response,
        tx_event,
        idle_timeout,
        thinking_policy,
    ));
    ResponseStream { rx_event }
}

#[derive(Default)]
pub(super) struct ClaudeStreamState {
    response_id: Option<String>,
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_reported_input_tokens: i64,
    cache_accounting_reported: bool,
    output_tokens: i64,
    message_text: String,
    message_open: bool,
    tool_blocks: BTreeMap<i64, ClaudeToolBlockState>,
}

#[derive(Default)]
pub(super) struct ClaudeToolBlockState {
    call_id: Option<String>,
    name: Option<String>,
    initial_input: Option<Value>,
    partial_json: String,
}

#[derive(Default)]
pub(super) struct CommonStreamState {
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
pub(super) struct CommonToolCallState {
    pub(super) call_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) arguments: String,
    pub(super) provider_metadata: Option<Value>,
}

#[derive(Default)]
pub(super) struct CommonThinkTagStreamState {
    pub(super) mode: CommonThinkTagMode,
    pub(super) pending: String,
    pub(super) saw_tag: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommonThinkTagMode {
    #[default]
    Text,
    Reasoning,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CommonThinkTag {
    Open,
    Close,
}

pub(super) enum CommonThinkSegment {
    Text(String),
    Reasoning(String),
}

impl CommonThinkTagStreamState {
    pub(super) fn push(&mut self, text: &str) -> Vec<CommonThinkSegment> {
        self.pending.push_str(text);
        self.drain(false)
    }

    pub(super) fn finish(&mut self) -> Vec<CommonThinkSegment> {
        self.drain(true)
    }

    fn drain(&mut self, finish: bool) -> Vec<CommonThinkSegment> {
        let mut segments = Vec::new();
        loop {
            match self.mode {
                CommonThinkTagMode::Text => {
                    let Some((index, tag, tag_len)) = find_common_think_tag(&self.pending) else {
                        if let Some(text) = self.take_pending_text_prefix(finish) {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Text(text),
                            );
                        }
                        break;
                    };

                    let prefix = self.pending[..index].to_string();
                    self.pending.drain(..index + tag_len);
                    self.saw_tag = true;
                    match tag {
                        CommonThinkTag::Open => {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Text(prefix),
                            );
                            self.mode = CommonThinkTagMode::Reasoning;
                        }
                        CommonThinkTag::Close => {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Reasoning(prefix),
                            );
                            self.mode = CommonThinkTagMode::Text;
                        }
                    }
                }
                CommonThinkTagMode::Reasoning => {
                    let Some(index) =
                        find_ascii_case_insensitive(&self.pending, COMMON_THINK_CLOSE_TAG)
                    else {
                        if let Some(text) = self.take_pending_reasoning_prefix(finish) {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Reasoning(text),
                            );
                        }
                        break;
                    };

                    let prefix = self.pending[..index].to_string();
                    self.pending.drain(..index + COMMON_THINK_CLOSE_TAG.len());
                    self.saw_tag = true;
                    push_common_think_segment(&mut segments, CommonThinkSegment::Reasoning(prefix));
                    self.mode = CommonThinkTagMode::Text;
                }
            }
        }
        segments
    }

    fn take_pending_text_prefix(&mut self, finish: bool) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        if finish {
            return Some(std::mem::take(&mut self.pending));
        }
        if !self.saw_tag && self.pending.len() <= COMMON_THINK_PRELUDE_BUFFER_BYTES {
            return None;
        }
        self.take_safe_pending_prefix()
    }

    fn take_pending_reasoning_prefix(&mut self, finish: bool) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        if finish {
            return Some(std::mem::take(&mut self.pending));
        }
        self.take_safe_pending_prefix()
    }

    fn take_safe_pending_prefix(&mut self) -> Option<String> {
        if self.pending.len() <= COMMON_THINK_TAG_TAIL_BYTES {
            return None;
        }
        let prefix_len = floor_char_boundary(
            self.pending.as_str(),
            self.pending.len() - COMMON_THINK_TAG_TAIL_BYTES,
        );
        if prefix_len == 0 {
            return None;
        }
        Some(self.pending.drain(..prefix_len).collect())
    }
}

pub(super) fn push_common_think_segment(
    segments: &mut Vec<CommonThinkSegment>,
    segment: CommonThinkSegment,
) {
    let is_empty = match &segment {
        CommonThinkSegment::Text(text) | CommonThinkSegment::Reasoning(text) => text.is_empty(),
    };
    if !is_empty {
        segments.push(segment);
    }
}

pub(super) fn find_common_think_tag(text: &str) -> Option<(usize, CommonThinkTag, usize)> {
    let open = find_ascii_case_insensitive(text, COMMON_THINK_OPEN_TAG)
        .map(|index| (index, CommonThinkTag::Open, COMMON_THINK_OPEN_TAG.len()));
    let close = find_ascii_case_insensitive(text, COMMON_THINK_CLOSE_TAG)
        .map(|index| (index, CommonThinkTag::Close, COMMON_THINK_CLOSE_TAG.len()));
    match (open, close) {
        (Some(open), Some(close)) => Some(if open.0 <= close.0 { open } else { close }),
        (Some(tag), None) | (None, Some(tag)) => Some(tag),
        (None, None) => None,
    }
}

pub(super) fn find_ascii_case_insensitive(text: &str, needle: &str) -> Option<usize> {
    text.to_ascii_lowercase().find(needle)
}

pub(super) fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut index = index;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

pub(super) async fn process_claude_sse(
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
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        format!("claude stream error: {err}"),
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
            Err(err) => {
                let _ = tx_event
                    .send(Err(PraxisErr::Stream(
                        format!("invalid claude stream event: {err}"),
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

pub(super) async fn process_common_sse(
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

pub(super) async fn process_claude_stream_event(
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
            let message = event.get("message");
            if let Some(response_id) = message
                .and_then(|message| message.get("id"))
                .and_then(Value::as_str)
            {
                state.response_id = Some(response_id.to_string());
            }
            update_claude_usage(state, message.and_then(|message| message.get("usage")));
        }
        "content_block_start" => {
            let index = event.get("index").and_then(Value::as_i64).unwrap_or(0);
            let Some(block) = event.get("content_block") else {
                return Ok(false);
            };
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        emit_claude_text_delta(state, tx_event, text).await?;
                    }
                }
                Some("tool_use") => {
                    emit_claude_message_done(state, tx_event).await?;
                    let entry = state.tool_blocks.entry(index).or_default();
                    entry.call_id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| Some(format!("claude-tool-{index}-{}", Uuid::new_v4())));
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
                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                        emit_claude_text_delta(state, tx_event, text).await?;
                    }
                }
                Some("input_json_delta") => {
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
            emit_claude_tool_done(state, tx_event, index).await?;
        }
        "message_delta" => {
            update_claude_usage(state, event.get("usage"));
        }
        "message_stop" => {
            emit_claude_message_done(state, tx_event).await?;
            let tool_indexes = state.tool_blocks.keys().copied().collect::<Vec<_>>();
            for index in tool_indexes {
                emit_claude_tool_done(state, tx_event, index).await?;
            }
            let response_id = state
                .response_id
                .clone()
                .unwrap_or_else(|| format!("claude-{}", Uuid::new_v4()));
            let token_usage = Some(TokenUsage {
                input_tokens: state.input_tokens,
                cached_input_tokens: state.cached_input_tokens,
                cache_reported_input_tokens: state.cache_reported_input_tokens,
                output_tokens: state.output_tokens,
                reasoning_output_tokens: 0,
                total_tokens: state.input_tokens + state.output_tokens,
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
            let message = event
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("claude stream error");
            return Err(PraxisErr::Stream(message.to_string(), None));
        }
        "ping" => {}
        _ => {}
    }

    Ok(false)
}

pub(super) async fn process_common_stream_event(
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
    if let Some(usage) = parse_common_usage(chunk.get("usage")) {
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

pub(super) async fn emit_claude_text_delta(
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

pub(super) async fn emit_claude_message_done(
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

pub(super) async fn emit_claude_tool_done(
    state: &mut ClaudeStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    index: i64,
) -> Result<()> {
    let Some(tool) = state.tool_blocks.remove(&index) else {
        return Ok(());
    };
    let name = tool.name.unwrap_or_else(|| format!("claude_tool_{index}"));
    let call_id = tool
        .call_id
        .unwrap_or_else(|| format!("claude-tool-{index}-{}", Uuid::new_v4()));
    let input = finalize_claude_tool_input(tool.initial_input, &tool.partial_json);
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

pub(super) fn finalize_claude_tool_input(
    initial_input: Option<Value>,
    partial_json: &str,
) -> Value {
    if !partial_json.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(partial_json) {
            return value;
        }
        return json!({ "input": partial_json });
    }

    initial_input.unwrap_or_else(|| json!({}))
}

pub(super) fn update_claude_usage(state: &mut ClaudeStreamState, usage: Option<&Value>) {
    let Some(usage) = usage else {
        return;
    };
    if let Some(input_tokens) = usage.get("input_tokens").and_then(Value::as_i64) {
        state.input_tokens = input_tokens;
    }
    if let Some(cached_input_tokens) = usage.get("cache_read_input_tokens").and_then(Value::as_i64)
    {
        state.cached_input_tokens = cached_input_tokens;
    }
    if usage.get("cache_read_input_tokens").is_some()
        || usage.get("cache_creation_input_tokens").is_some()
    {
        state.cache_accounting_reported = true;
        state.cache_reported_input_tokens = state.input_tokens.max(0);
    }
    if state.cache_accounting_reported {
        state.cache_reported_input_tokens = state.input_tokens.max(0);
    }
    if let Some(output_tokens) = usage.get("output_tokens").and_then(Value::as_i64) {
        state.output_tokens = output_tokens;
    }
}

pub(super) async fn emit_common_text_delta(
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

pub(super) async fn emit_common_content_delta(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    delta: &str,
) -> Result<()> {
    let segments = state.think_tag_parser.push(delta);
    emit_common_content_segments(state, tx_event, segments).await
}

pub(super) async fn emit_common_content_done(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    let segments = state.think_tag_parser.finish();
    emit_common_content_segments(state, tx_event, segments).await
}

pub(super) async fn emit_common_content_segments(
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

pub(super) async fn emit_common_reasoning_delta(
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

pub(super) fn common_next_wait_timeout(
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

pub(super) fn common_should_complete_now(
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
pub(super) fn common_stream_produced_output(state: &CommonStreamState) -> bool {
    !state.message_text.is_empty()
        || !state.reasoning_text.is_empty()
        || !state.tool_calls.is_empty()
        || state.tool_calls_emitted
}

pub(super) fn common_can_complete_on_timeout(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    (state.saw_finish_reason && common_stream_produced_output(state))
        || common_can_complete_on_message_idle(state, thinking_policy)
}

pub(super) fn common_can_complete_on_stream_close(
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

pub(super) fn common_completion_deadline(
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

pub(super) fn common_can_complete_on_message_idle(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    thinking_policy.complete_on_message_idle
        && state.message_open
        && !state.message_text.is_empty()
        && state.tool_calls.is_empty()
}

pub(super) async fn emit_common_reasoning_done(
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

pub(super) async fn emit_common_message_done(
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

pub(super) async fn emit_common_tool_calls(
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

pub(super) async fn emit_common_completion(
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

pub(super) fn extract_common_stream_delta_text(content: Option<&Value>) -> Option<String> {
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

pub(super) fn value_is_empty_object(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.is_empty())
}

pub(super) async fn send_stream_event(
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    event: ResponseEvent,
) -> Result<()> {
    tx_event
        .send(Ok(event))
        .await
        .map_err(|err| PraxisErr::Fatal(format!("failed to emit provider stream event: {err}")))
}

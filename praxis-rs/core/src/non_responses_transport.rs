use crate::api_bridge::CoreAuthProvider;
use crate::api_bridge::map_api_error;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::error::PraxisErr;
use crate::error::Result;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::ModelProviderMaxTokensField;
use crate::model_provider_info::ModelProviderReasoningEffortMap;
use crate::model_provider_info::ModelProviderThinkingFormat;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::HeaderMap;
use http::HeaderValue;
use http::header::AUTHORIZATION;
use http::header::CONTENT_TYPE;
use praxis_api::AuthProvider as ApiAuthProvider;
use praxis_api::Provider;
use praxis_api::TransportError;
use praxis_api::error::ApiError;
use praxis_login::default_client::build_direct_reqwest_client;
use praxis_login::default_client::build_reqwest_client;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::LocalShellAction;
use praxis_protocol::models::ReasoningItemContent;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::protocol::TokenUsage;
use praxis_tools::JsonSchema;
use praxis_tools::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

const CLAUDE_API_VERSION: &str = "2023-06-01";
const DEFAULT_CLAUDE_MAX_TOKENS: i64 = 4096;
const COMMON_TOOL_RESULT_BRIDGE_MESSAGE: &str = "I have processed the tool results.";
const COMMON_POST_FINISH_GRACE_MS: u64 = 1_500;
const COMMON_DEEPSEEK_MESSAGE_IDLE_GRACE_MS: u64 = 15_000;
const COMMON_THINK_OPEN_TAG: &str = "<think>";
const COMMON_THINK_CLOSE_TAG: &str = "</think>";
const COMMON_THINK_TAG_TAIL_BYTES: usize = COMMON_THINK_CLOSE_TAG.len() - 1;
const COMMON_THINK_PRELUDE_BUFFER_BYTES: usize = 128;

pub(crate) async fn stream_claude_unary(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    prompt: &Prompt,
    model_info: &ModelInfo,
) -> Result<ResponseStream> {
    let request_body = build_claude_request(prompt, model_info, true)?;
    let response = send_request(
        &api_provider,
        &api_auth,
        build_claude_endpoint_path(&api_provider),
        &request_body,
        RequestFamily::Claude,
        ProviderTransportPolicy::SystemProxy,
    )
    .await?;

    if response_is_sse(&response) {
        return Ok(spawn_claude_sse_stream(
            response,
            api_provider.stream_idle_timeout,
        ));
    }

    let response_json = read_json_response(response).await?;
    build_response_stream(parse_claude_response(response_json)?)
}

pub(crate) async fn stream_common_unary(
    api_provider: Provider,
    api_auth: CoreAuthProvider,
    provider_info: &ModelProviderInfo,
    prompt: &Prompt,
    model_info: &ModelInfo,
    effort: Option<ReasoningEffortConfig>,
) -> Result<ResponseStream> {
    let common_compat = CommonRequestCompat::from_provider_and_model(provider_info, model_info);
    let thinking_policy = CommonThinkingPolicy::from_format(common_compat.thinking_format);
    let request_body = build_common_request(prompt, model_info, provider_info, effort, true)?;
    let response = send_request(
        &api_provider,
        &api_auth,
        build_common_endpoint_path(&api_provider),
        &request_body,
        RequestFamily::Common,
        ProviderTransportPolicy::from_model_provider(provider_info),
    )
    .await?;

    if response_is_sse(&response) {
        return Ok(spawn_common_sse_stream(
            response,
            api_provider.stream_idle_timeout,
            thinking_policy,
        ));
    }

    let response_json = read_json_response(response).await?;
    build_response_stream(parse_common_response(response_json, thinking_policy)?)
}

#[derive(Clone, Copy)]
enum RequestFamily {
    Claude,
    Common,
}

#[derive(Clone, Copy)]
enum ProviderTransportPolicy {
    SystemProxy,
    Direct,
}

impl ProviderTransportPolicy {
    fn from_model_provider(provider_info: &ModelProviderInfo) -> Self {
        if provider_info.is_openai() || provider_info.has_command_auth() {
            Self::SystemProxy
        } else {
            Self::Direct
        }
    }
}

struct ParsedProviderResponse {
    response_id: String,
    token_usage: Option<TokenUsage>,
    items: Vec<ResponseItem>,
}

async fn send_request(
    api_provider: &Provider,
    api_auth: &CoreAuthProvider,
    endpoint_path: &str,
    request_body: &Value,
    family: RequestFamily,
    transport_policy: ProviderTransportPolicy,
) -> Result<reqwest::Response> {
    let client = match transport_policy {
        ProviderTransportPolicy::SystemProxy => build_reqwest_client(),
        ProviderTransportPolicy::Direct => build_direct_reqwest_client(),
    };
    let url = api_provider.url_for_path(endpoint_path);
    let headers = build_request_headers(api_provider, api_auth, family)?;

    let response = client
        .post(url.clone())
        .headers(headers)
        .json(request_body)
        .send()
        .await
        .map_err(map_reqwest_error)?;

    let status = response.status();
    if !status.is_success() {
        let response_url = response.url().to_string();
        let response_headers = response.headers().clone();
        let body = response.text().await.map_err(map_reqwest_error)?;
        let transport = TransportError::Http {
            status,
            url: Some(response_url),
            headers: Some(response_headers),
            body: Some(body),
        };
        return Err(map_api_error(ApiError::Transport(transport)));
    }

    Ok(response)
}

async fn read_json_response(response: reqwest::Response) -> Result<Value> {
    let body = response.text().await.map_err(map_reqwest_error)?;
    serde_json::from_str(&body).map_err(PraxisErr::from)
}

fn build_request_headers(
    api_provider: &Provider,
    api_auth: &CoreAuthProvider,
    family: RequestFamily,
) -> Result<HeaderMap> {
    let mut headers = api_provider.headers.clone();

    match family {
        RequestFamily::Claude => {
            insert_header_if_missing(&mut headers, "anthropic-version", CLAUDE_API_VERSION)?;
            attach_token_if_missing(&mut headers, api_auth, TokenHeaderMode::ClaudeApiKey)?;
        }
        RequestFamily::Common => {
            attach_token_if_missing(&mut headers, api_auth, TokenHeaderMode::Bearer)?;
        }
    }

    Ok(headers)
}

enum TokenHeaderMode {
    Bearer,
    ClaudeApiKey,
}

fn attach_token_if_missing(
    headers: &mut HeaderMap,
    api_auth: &CoreAuthProvider,
    mode: TokenHeaderMode,
) -> Result<()> {
    let Some(token) = api_auth.bearer_token() else {
        return Ok(());
    };

    if headers.contains_key(AUTHORIZATION) || headers.contains_key("x-api-key") {
        return Ok(());
    }

    match mode {
        TokenHeaderMode::Bearer => {
            let value = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|err| {
                PraxisErr::InvalidRequest(format!("failed to encode bearer token header: {err}"))
            })?;
            headers.insert(AUTHORIZATION, value);
        }
        TokenHeaderMode::ClaudeApiKey => {
            insert_header_if_missing(headers, "x-api-key", &token)?;
        }
    }

    Ok(())
}

fn insert_header_if_missing(headers: &mut HeaderMap, key: &str, value: &str) -> Result<()> {
    if headers.contains_key(key) {
        return Ok(());
    }
    let header_name: http::header::HeaderName = key.parse().map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "failed to parse provider header name `{key}`: {err}"
        ))
    })?;
    let header_value = HeaderValue::from_str(value).map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "failed to parse provider header `{key}` value: {err}"
        ))
    })?;
    headers.insert(header_name, header_value);
    Ok(())
}

fn build_claude_endpoint_path(api_provider: &Provider) -> &'static str {
    let base = api_provider
        .base_url
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if base.ends_with("/messages") {
        ""
    } else if base.ends_with("/v1") {
        "messages"
    } else {
        "v1/messages"
    }
}

fn build_common_endpoint_path(api_provider: &Provider) -> &'static str {
    let base = api_provider
        .base_url
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if base.ends_with("/chat/completions") {
        ""
    } else if base.ends_with("/v1") {
        "chat/completions"
    } else {
        "v1/chat/completions"
    }
}

fn build_claude_request(prompt: &Prompt, model_info: &ModelInfo, stream: bool) -> Result<Value> {
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

fn build_common_request(
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
struct CommonRequestCompat {
    supports_developer_role: bool,
    supports_reasoning_effort: bool,
    reasoning_effort_map: Option<ModelProviderReasoningEffortMap>,
    max_tokens_field: Option<ModelProviderMaxTokensField>,
    thinking_format: ModelProviderThinkingFormat,
    requires_tool_result_name: bool,
    requires_assistant_after_tool_result: bool,
    emit_parallel_tool_calls: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommonThinkingRequestStyle {
    ReasoningEffortField,
    OpenRouterReasoningObject,
    EnableThinkingBool,
    ZaiThinkingObject,
    QwenChatTemplateKwargs,
}

#[derive(Debug, Clone, Copy)]
struct CommonThinkingPolicy {
    request_style: CommonThinkingRequestStyle,
    replay_field: Option<&'static str>,
    response_fields: &'static [&'static str],
    complete_on_message_idle: bool,
    complete_on_finish_reason: bool,
}

impl CommonThinkingPolicy {
    fn from_format(format: ModelProviderThinkingFormat) -> Self {
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
            requires_tool_result_name: false,
            requires_assistant_after_tool_result: false,
            emit_parallel_tool_calls: true,
        }
    }
}

impl CommonRequestCompat {
    fn from_provider_and_model(provider_info: &ModelProviderInfo, model_info: &ModelInfo) -> Self {
        let inferred_compat = infer_common_request_compat(
            provider_info.base_url.as_deref(),
            model_info.slug.as_str(),
        );
        let compat = Some(merge_common_request_compat(
            inferred_compat,
            provider_info.compat.clone(),
        ));
        let compat = compat.as_ref();
        Self {
            supports_developer_role: compat
                .and_then(|compat| compat.supports_developer_role)
                .unwrap_or(false),
            supports_reasoning_effort: compat
                .and_then(|compat| compat.supports_reasoning_effort)
                .unwrap_or(false),
            reasoning_effort_map: compat.and_then(|compat| compat.reasoning_effort_map.clone()),
            max_tokens_field: compat.and_then(|compat| compat.max_tokens_field),
            thinking_format: compat
                .and_then(|compat| compat.thinking_format)
                .unwrap_or(ModelProviderThinkingFormat::Openai),
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

fn infer_common_request_compat(
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
    compat
}

fn merge_common_request_compat(
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
enum CommonHistoryRole {
    User,
    Assistant,
    ToolResult,
}

fn build_common_messages(
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

        if let Some(tool_call) = response_item_to_common_tool_call(item, &mut tool_names_by_call_id)
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

fn push_common_text_message(messages: &mut Vec<Value>, role: &str, content: &str) {
    let content = content.trim();
    if content.is_empty() {
        return;
    }

    messages.push(json!({
        "role": role,
        "content": content,
    }));
}

fn next_common_history_role(items: &[ResponseItem]) -> Option<CommonHistoryRole> {
    items.iter().find_map(common_history_role)
}

fn next_common_item_is_tool_call(items: &[ResponseItem]) -> bool {
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

fn common_reasoning_content(item: &ResponseItem) -> Option<String> {
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

fn append_common_reasoning_content(target: &mut String, content: &str) {
    if content.trim().is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(content);
}

fn attach_common_reasoning_content(
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

fn ensure_common_assistant_message(pending_assistant_message: &mut Option<Value>) -> &mut Value {
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

fn append_common_tool_call(message: &mut Value, tool_call: Value) {
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

fn flush_common_assistant_message(
    messages: &mut Vec<Value>,
    pending_assistant_message: &mut Option<Value>,
) {
    if let Some(message) = pending_assistant_message.take() {
        messages.push(message);
    }
}

fn collect_system_prompt(prompt: &Prompt, items: &[ResponseItem]) -> String {
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

fn response_item_to_claude_message(item: &ResponseItem) -> Option<Value> {
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

fn response_item_to_common_message(
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
                            "arguments": normalize_function_arguments_string(arguments),
                        }
                    }],
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

fn response_item_is_common_tool_call(item: &ResponseItem) -> bool {
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

fn response_item_to_common_tool_call(
    item: &ResponseItem,
    tool_names_by_call_id: &mut BTreeMap<String, String>,
) -> Option<Value> {
    match item {
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            ..
        } => {
            tool_names_by_call_id.insert(call_id.clone(), name.clone());
            Some(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": normalize_function_arguments_string(arguments),
                }
            }))
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

fn common_history_role(item: &ResponseItem) -> Option<CommonHistoryRole> {
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

fn common_tool_result_message(
    call_id: &str,
    output: &FunctionCallOutputPayload,
    compat: &CommonRequestCompat,
    tool_name: Option<&str>,
) -> Value {
    common_tool_result_message_from_string(call_id, output.to_string(), compat, tool_name)
}

fn common_tool_result_message_from_string(
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

fn apply_common_reasoning_config(
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

fn map_common_reasoning_effort(
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

fn reasoning_effort_enables_thinking(effort: Option<ReasoningEffortConfig>) -> bool {
    matches!(
        effort,
        Some(reasoning_effort) if reasoning_effort != ReasoningEffortConfig::None
    )
}

fn common_max_tokens_field_name(field: ModelProviderMaxTokensField) -> &'static str {
    match field {
        ModelProviderMaxTokensField::MaxCompletionTokens => "max_completion_tokens",
        ModelProviderMaxTokensField::MaxTokens => "max_tokens",
    }
}

fn tool_result_to_claude_message(
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

fn claude_content_blocks(content: &[ContentItem]) -> Vec<Value> {
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

fn common_message_content(content: &[ContentItem]) -> Value {
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

fn render_text_only_content(content: &[ContentItem]) -> String {
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

fn parse_data_url(image_url: &str) -> Option<(String, String)> {
    let image_url = image_url.strip_prefix("data:")?;
    let (metadata, data) = image_url.split_once(",")?;
    let media_type = metadata.strip_suffix(";base64")?;
    Some((media_type.to_string(), data.to_string()))
}

fn tool_spec_to_claude_tool(tool: &ToolSpec) -> Option<Value> {
    let function = tool_spec_to_function_definition(tool)?;
    Some(json!({
        "name": function.name,
        "description": function.description,
        "input_schema": function.parameters,
    }))
}

fn tool_spec_to_common_tool(tool: &ToolSpec) -> Option<Value> {
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

struct ProviderFunctionTool {
    name: String,
    description: String,
    parameters: Value,
}

fn tool_spec_to_function_definition(tool: &ToolSpec) -> Option<ProviderFunctionTool> {
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

fn local_shell_schema() -> JsonSchema {
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

fn freeform_tool_schema() -> JsonSchema {
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

fn web_search_schema() -> JsonSchema {
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

fn local_shell_exec_json(exec: &praxis_protocol::models::LocalShellExecAction) -> Value {
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

fn normalize_function_arguments(arguments: &str) -> Value {
    match serde_json::from_str::<Value>(arguments) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(other) => json!({ "value": other }),
        Err(_) => json!({ "input": arguments }),
    }
}

fn normalize_function_arguments_string(arguments: &str) -> String {
    match serde_json::from_str::<Value>(arguments) {
        Ok(value) => serde_json::to_string(&value).unwrap_or_else(|_| arguments.to_string()),
        Err(_) => serde_json::to_string(&json!({ "input": arguments }))
            .unwrap_or_else(|_| arguments.to_string()),
    }
}

fn response_is_sse(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
}

fn spawn_claude_sse_stream(response: reqwest::Response, idle_timeout: Duration) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(256);
    tokio::spawn(process_claude_sse(response, tx_event, idle_timeout));
    ResponseStream { rx_event }
}

fn spawn_common_sse_stream(
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
struct ClaudeStreamState {
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
struct ClaudeToolBlockState {
    call_id: Option<String>,
    name: Option<String>,
    initial_input: Option<Value>,
    partial_json: String,
}

#[derive(Default)]
struct CommonStreamState {
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
struct CommonToolCallState {
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Default)]
struct CommonThinkTagStreamState {
    mode: CommonThinkTagMode,
    pending: String,
    saw_tag: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum CommonThinkTagMode {
    #[default]
    Text,
    Reasoning,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommonThinkTag {
    Open,
    Close,
}

enum CommonThinkSegment {
    Text(String),
    Reasoning(String),
}

impl CommonThinkTagStreamState {
    fn push(&mut self, text: &str) -> Vec<CommonThinkSegment> {
        self.pending.push_str(text);
        self.drain(false)
    }

    fn finish(&mut self) -> Vec<CommonThinkSegment> {
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

fn push_common_think_segment(segments: &mut Vec<CommonThinkSegment>, segment: CommonThinkSegment) {
    let is_empty = match &segment {
        CommonThinkSegment::Text(text) | CommonThinkSegment::Reasoning(text) => text.is_empty(),
    };
    if !is_empty {
        segments.push(segment);
    }
}

fn find_common_think_tag(text: &str) -> Option<(usize, CommonThinkTag, usize)> {
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

fn find_ascii_case_insensitive(text: &str, needle: &str) -> Option<usize> {
    text.to_ascii_lowercase().find(needle)
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut index = index;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

async fn process_claude_sse(
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

async fn process_common_sse(
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

async fn process_claude_stream_event(
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

async fn process_common_stream_event(
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

async fn emit_claude_text_delta(
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

async fn emit_claude_message_done(
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

async fn emit_claude_tool_done(
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
            name,
            namespace: None,
            arguments: serde_json::to_string(&input)?,
            call_id,
        }),
    )
    .await
}

fn finalize_claude_tool_input(initial_input: Option<Value>, partial_json: &str) -> Value {
    if !partial_json.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(partial_json) {
            return value;
        }
        return json!({ "input": partial_json });
    }

    initial_input.unwrap_or_else(|| json!({}))
}

fn update_claude_usage(state: &mut ClaudeStreamState, usage: Option<&Value>) {
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

async fn emit_common_text_delta(
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

async fn emit_common_content_delta(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    delta: &str,
) -> Result<()> {
    let segments = state.think_tag_parser.push(delta);
    emit_common_content_segments(state, tx_event, segments).await
}

async fn emit_common_content_done(
    state: &mut CommonStreamState,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    let segments = state.think_tag_parser.finish();
    emit_common_content_segments(state, tx_event, segments).await
}

async fn emit_common_content_segments(
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

async fn emit_common_reasoning_delta(
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

fn common_next_wait_timeout(
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

fn common_should_complete_now(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    common_completion_deadline(state, thinking_policy)
        .is_some_and(|deadline| Instant::now() >= deadline)
}

fn common_can_complete_on_timeout(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    state.saw_finish_reason || common_can_complete_on_message_idle(state, thinking_policy)
}

fn common_can_complete_on_stream_close(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    state.saw_finish_reason || common_can_complete_on_message_idle(state, thinking_policy)
}

fn common_completion_deadline(
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

fn common_can_complete_on_message_idle(
    state: &CommonStreamState,
    thinking_policy: CommonThinkingPolicy,
) -> bool {
    thinking_policy.complete_on_message_idle
        && state.message_open
        && !state.message_text.is_empty()
        && state.tool_calls.is_empty()
}

async fn emit_common_reasoning_done(
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

async fn emit_common_message_done(
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

async fn emit_common_tool_calls(
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

async fn emit_common_completion(
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

fn extract_common_stream_delta_text(content: Option<&Value>) -> Option<String> {
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

fn value_is_empty_object(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.is_empty())
}

async fn send_stream_event(
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    event: ResponseEvent,
) -> Result<()> {
    tx_event
        .send(Ok(event))
        .await
        .map_err(|err| PraxisErr::Fatal(format!("failed to emit provider stream event: {err}")))
}

fn parse_claude_response(response_json: Value) -> Result<ParsedProviderResponse> {
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

fn parse_common_response(
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

fn normalize_common_tool_call_name(name: Option<String>, arguments: &str) -> Option<String> {
    if let Some(name) = name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return Some(name.to_string());
    }

    infer_common_tool_call_name_from_arguments(arguments).map(str::to_string)
}

fn infer_common_tool_call_name_from_arguments(arguments: &str) -> Option<&'static str> {
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

fn extract_common_reasoning_content(
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

fn extract_common_reasoning_delta(
    delta: &Value,
    thinking_policy: CommonThinkingPolicy,
) -> Option<String> {
    thinking_policy
        .response_fields
        .iter()
        .find_map(|key| extract_common_stream_delta_text(delta.get(key)))
}

fn common_reasoning_item(text: String) -> ResponseItem {
    common_reasoning_item_with_id(format!("common-reasoning-{}", Uuid::new_v4()), text)
}

fn common_reasoning_item_with_id(id: String, text: String) -> ResponseItem {
    ResponseItem::Reasoning {
        id,
        summary: Vec::new(),
        content: Some(vec![ReasoningItemContent::ReasoningText { text }]),
        encrypted_content: None,
    }
}

fn push_common_message_item(items: &mut Vec<ResponseItem>, text: String) {
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

fn split_common_think_tag_segments(text: &str) -> Vec<CommonThinkSegment> {
    let mut parser = CommonThinkTagStreamState::default();
    parser.pending.push_str(text);
    parser.finish()
}

fn extract_common_response_text(content: Option<&Value>) -> Option<String> {
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

fn parse_claude_usage(usage: Option<&Value>) -> Option<TokenUsage> {
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

fn parse_common_usage(usage: Option<&Value>) -> Option<TokenUsage> {
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

fn value_to_json_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn build_response_stream(parsed: ParsedProviderResponse) -> Result<ResponseStream> {
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

fn map_reqwest_error(err: reqwest::Error) -> PraxisErr {
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_partial_json;
    use wiremock::matchers::header;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    fn model_info() -> ModelInfo {
        serde_json::from_value(json!({
            "slug": "test-model",
            "display_name": "Test Model",
            "description": null,
            "default_reasoning_level": null,
            "supported_reasoning_levels": [],
            "shell_type": "local",
            "visibility": "list",
            "supported_in_api": true,
            "priority": 0,
            "availability_nux": null,
            "upgrade": null,
            "base_instructions": "",
            "model_messages": null,
            "supports_reasoning_summaries": false,
            "default_reasoning_summary": "auto",
            "support_verbosity": false,
            "default_verbosity": null,
            "apply_patch_tool_type": null,
            "web_search_tool_type": "text",
            "truncation_policy": {
                "mode": "tokens",
                "limit": 100000
            },
            "supports_parallel_tool_calls": true,
            "supports_image_detail_original": false,
            "context_window": null,
            "auto_compact_token_limit": null,
            "effective_context_window_percent": 100,
            "experimental_supported_tools": [],
            "input_modalities": ["text"],
            "supports_search_tool": false
        }))
        .expect("test model info")
    }

    fn model_info_with_default_reasoning(
        default_reasoning_level: Option<ReasoningEffortConfig>,
    ) -> ModelInfo {
        let mut info = model_info();
        info.default_reasoning_level = default_reasoning_level;
        info
    }

    fn model_info_with_slug(slug: &str) -> ModelInfo {
        let mut info = model_info();
        info.slug = slug.to_string();
        info
    }

    fn provider(base_url: String) -> Provider {
        Provider {
            name: "test".to_string(),
            base_url,
            query_params: None,
            headers: HeaderMap::new(),
            retry: praxis_api::provider::RetryConfig {
                max_attempts: 1,
                base_delay: std::time::Duration::from_millis(1),
                retry_429: false,
                retry_5xx: false,
                retry_transport: false,
            },
            stream_idle_timeout: std::time::Duration::from_secs(30),
        }
    }

    fn common_provider_info(
        compat: Option<crate::model_provider_info::ModelProviderCompatInfo>,
    ) -> ModelProviderInfo {
        ModelProviderInfo {
            name: "Common Test Provider".to_string(),
            base_url: Some("https://example.com/v1".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: crate::model_provider_info::WireApi::OpenAiCompat,
            compat,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            websocket_connect_timeout_ms: None,
            requires_openai_auth: false,
            supports_websockets: false,
        }
    }

    #[test]
    fn common_request_can_add_tool_result_name_and_bridge_assistant_message() {
        let prompt = Prompt {
            input: vec![
                ResponseItem::FunctionCall {
                    id: None,
                    name: "apply_patch".to_string(),
                    namespace: None,
                    arguments: "{\"input\":\"*** Begin Patch\\n*** End Patch\\n\"}".to_string(),
                    call_id: "call_1".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call_1".to_string(),
                    output: FunctionCallOutputPayload::from_text("ok".to_string()),
                },
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "continue".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
            ],
            ..Prompt::default()
        };
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                requires_tool_result_name: Some(true),
                requires_assistant_after_tool_result: Some(true),
                ..Default::default()
            }));

        let request = build_common_request(&prompt, &model_info(), &provider, None, true)
            .expect("common request should build");

        let messages = request
            .get("messages")
            .and_then(Value::as_array)
            .expect("messages array");
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["name"], "apply_patch");
        assert_eq!(messages[3]["role"], "assistant");
        assert_eq!(messages[3]["content"], COMMON_TOOL_RESULT_BRIDGE_MESSAGE);
        assert_eq!(messages[4]["role"], "user");
        assert_eq!(messages[4]["content"], "continue");
    }

    #[test]
    fn common_request_groups_parallel_tool_calls_and_replays_reasoning_content() {
        let prompt = Prompt {
            input: vec![
                ResponseItem::Reasoning {
                    id: "reasoning_1".to_string(),
                    summary: Vec::new(),
                    content: Some(vec![ReasoningItemContent::ReasoningText {
                        text: "choose tools".to_string(),
                    }]),
                    encrypted_content: None,
                },
                ResponseItem::FunctionCall {
                    id: None,
                    name: "apply_patch".to_string(),
                    namespace: None,
                    arguments: "{\"patch\":\"*** Begin Patch\\n*** End Patch\\n\"}".to_string(),
                    call_id: "call_1".to_string(),
                },
                ResponseItem::FunctionCall {
                    id: None,
                    name: "shell".to_string(),
                    namespace: None,
                    arguments: "{\"command\":\"pwd\"}".to_string(),
                    call_id: "call_2".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call_1".to_string(),
                    output: FunctionCallOutputPayload::from_text("patch ok".to_string()),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call_2".to_string(),
                    output: FunctionCallOutputPayload::from_text("shell ok".to_string()),
                },
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "continue".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
            ],
            ..Prompt::default()
        };

        let request = build_common_request(
            &prompt,
            &model_info(),
            &common_provider_info(None),
            None,
            true,
        )
        .expect("common request should build");

        let messages = request["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["reasoning_content"], "choose tools");
        let tool_calls = messages[1]["tool_calls"].as_array().expect("tool calls");
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(tool_calls[1]["id"], "call_2");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_1");
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "call_2");
        assert_eq!(messages[4]["role"], "user");
    }

    #[test]
    fn common_request_does_not_replay_deepseek_reasoning_content() {
        let prompt = Prompt {
            input: vec![
                ResponseItem::Reasoning {
                    id: "reasoning_1".to_string(),
                    summary: Vec::new(),
                    content: Some(vec![ReasoningItemContent::ReasoningText {
                        text: "choose tools".to_string(),
                    }]),
                    encrypted_content: None,
                },
                ResponseItem::FunctionCall {
                    id: None,
                    name: "apply_patch".to_string(),
                    namespace: None,
                    arguments: "{\"patch\":\"*** Begin Patch\\n*** End Patch\\n\"}".to_string(),
                    call_id: "call_1".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call_1".to_string(),
                    output: FunctionCallOutputPayload::from_text("patch ok".to_string()),
                },
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "continue".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
            ],
            ..Prompt::default()
        };
        let mut provider = common_provider_info(None);
        provider.base_url = Some("https://api.deepseek.com".to_string());

        let request = build_common_request(&prompt, &model_info(), &provider, None, true)
            .expect("common request should build");

        let messages = request["messages"].as_array().expect("messages array");
        assert_eq!(messages[1]["role"], "assistant");
        assert!(messages[1].get("reasoning_content").is_none());
        assert!(messages[1]["tool_calls"].as_array().is_some());
    }

    #[test]
    fn common_request_merges_assistant_text_reasoning_and_tool_calls() {
        let prompt = Prompt {
            input: vec![
                ResponseItem::Reasoning {
                    id: "reasoning_1".to_string(),
                    summary: Vec::new(),
                    content: Some(vec![ReasoningItemContent::ReasoningText {
                        text: "need shell".to_string(),
                    }]),
                    encrypted_content: None,
                },
                ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: "I will inspect the workspace.".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
                ResponseItem::FunctionCall {
                    id: None,
                    name: "local_shell".to_string(),
                    namespace: None,
                    arguments: "{\"command\":[\"pwd\"]}".to_string(),
                    call_id: "call_1".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call_1".to_string(),
                    output: FunctionCallOutputPayload::from_text("D:\\ghost1.0".to_string()),
                },
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "continue".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
            ],
            ..Prompt::default()
        };

        let request = build_common_request(
            &prompt,
            &model_info(),
            &common_provider_info(None),
            None,
            true,
        )
        .expect("common request should build");

        let messages = request["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"], "I will inspect the workspace.");
        assert_eq!(messages[1]["reasoning_content"], "need shell");
        let tool_calls = messages[1]["tool_calls"].as_array().expect("tool calls");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_1");
        assert_eq!(messages[3]["role"], "user");
    }

    #[test]
    fn common_request_can_omit_parallel_tool_calls_via_provider_compat() {
        let prompt = Prompt {
            tools: vec![ToolSpec::Function(praxis_tools::ResponsesApiTool {
                name: "echo".to_string(),
                description: "Echo text".to_string(),
                strict: false,
                defer_loading: None,
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "text".to_string(),
                        JsonSchema::String { description: None },
                    )]),
                    required: None,
                    additional_properties: None,
                },
                output_schema: None,
            })],
            parallel_tool_calls: true,
            ..Prompt::default()
        };
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                supports_parallel_tool_calls: Some(false),
                ..Default::default()
            }));

        let request = build_common_request(&prompt, &model_info(), &provider, None, true)
            .expect("common request should build");

        assert!(request.get("tools").is_some());
        assert!(request.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn common_request_can_preserve_developer_role_messages_when_supported() {
        let prompt = Prompt {
            base_instructions: praxis_protocol::models::BaseInstructions {
                text: "base prompt".to_string(),
            },
            input: vec![
                ResponseItem::Message {
                    id: None,
                    role: "system".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "system note".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
                ResponseItem::Message {
                    id: None,
                    role: "developer".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "developer note".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "hello".to_string(),
                    }],
                    end_turn: None,
                    phase: None,
                },
            ],
            ..Prompt::default()
        };
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                supports_developer_role: Some(true),
                ..Default::default()
            }));

        let request = build_common_request(&prompt, &model_info(), &provider, None, true)
            .expect("common request should build");

        let messages = request
            .get("messages")
            .and_then(Value::as_array)
            .expect("messages array");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "developer");
        assert_eq!(messages[0]["content"], "base prompt");
        assert_eq!(messages[1]["role"], "system");
        assert_eq!(messages[1]["content"], "system note");
        assert_eq!(messages[2]["role"], "developer");
        assert_eq!(messages[2]["content"], "developer note");
        assert_eq!(messages[3]["role"], "user");
        assert_eq!(messages[3]["content"], "hello");
    }

    #[test]
    fn common_request_can_emit_openai_reasoning_and_selected_max_tokens_field() {
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                supports_reasoning_effort: Some(true),
                reasoning_effort_map: Some(
                    crate::model_provider_info::ModelProviderReasoningEffortMap {
                        high: Some("max".to_string()),
                        ..Default::default()
                    },
                ),
                max_tokens_field: Some(
                    crate::model_provider_info::ModelProviderMaxTokensField::MaxCompletionTokens,
                ),
                ..Default::default()
            }));

        let request = build_common_request(
            &Prompt::default(),
            &model_info(),
            &provider,
            Some(ReasoningEffortConfig::High),
            true,
        )
        .expect("common request should build");

        assert_eq!(request["reasoning_effort"], "max");
        assert_eq!(request["max_completion_tokens"], 4096);
        assert!(request.get("max_tokens").is_none());
    }

    #[test]
    fn common_request_does_not_emit_provider_specific_thinking_object() {
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                supports_reasoning_effort: Some(true),
                reasoning_effort_map: Some(
                    crate::model_provider_info::ModelProviderReasoningEffortMap {
                        xhigh: Some("max".to_string()),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            }));

        let request = build_common_request(
            &Prompt::default(),
            &model_info(),
            &provider,
            Some(ReasoningEffortConfig::XHigh),
            true,
        )
        .expect("common request should build");

        assert!(request.get("thinking").is_none());
        assert_eq!(request["reasoning_effort"], "max");
    }

    #[test]
    fn common_request_uses_generic_reasoning_effort_for_non_openai_base_url() {
        let mut provider = common_provider_info(None);
        provider.base_url = Some("https://api.deepseek.com".to_string());

        let request = build_common_request(
            &Prompt::default(),
            &model_info(),
            &provider,
            Some(ReasoningEffortConfig::High),
            true,
        )
        .expect("common request should build");

        assert!(request.get("thinking").is_none());
        assert_eq!(request["reasoning_effort"], "high");
    }

    #[test]
    fn common_request_can_disable_generic_reasoning_effort() {
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                supports_reasoning_effort: Some(true),
                ..Default::default()
            }));

        let request = build_common_request(
            &Prompt::default(),
            &model_info(),
            &provider,
            Some(ReasoningEffortConfig::None),
            true,
        )
        .expect("common request should build");

        assert!(request.get("thinking").is_none());
        assert_eq!(request["reasoning_effort"], "none");
    }

    #[test]
    fn common_request_can_use_model_default_reasoning_for_zai_thinking_object() {
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                thinking_format: Some(crate::model_provider_info::ModelProviderThinkingFormat::Zai),
                ..Default::default()
            }));

        let request = build_common_request(
            &Prompt::default(),
            &model_info_with_default_reasoning(Some(ReasoningEffortConfig::Medium)),
            &provider,
            None,
            true,
        )
        .expect("common request should build");

        assert_eq!(request["thinking"]["type"], "enabled");
        assert!(request.get("enable_thinking").is_none());
    }

    #[test]
    fn common_request_uses_glm_model_slug_for_zai_thinking_object() {
        let provider = common_provider_info(None);

        let request = build_common_request(
            &Prompt::default(),
            &model_info_with_slug("glm-5.1"),
            &provider,
            Some(ReasoningEffortConfig::High),
            true,
        )
        .expect("common request should build");

        assert_eq!(request["thinking"]["type"], "enabled");
        assert!(request.get("enable_thinking").is_none());
        assert!(request.get("reasoning_effort").is_none());
    }

    #[test]
    fn common_request_can_disable_zai_thinking_object_with_explicit_none_effort() {
        let provider =
            common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
                thinking_format: Some(crate::model_provider_info::ModelProviderThinkingFormat::Zai),
                ..Default::default()
            }));

        let request = build_common_request(
            &Prompt::default(),
            &model_info_with_default_reasoning(Some(ReasoningEffortConfig::High)),
            &provider,
            Some(ReasoningEffortConfig::None),
            true,
        )
        .expect("common request should build");

        assert_eq!(request["thinking"]["type"], "disabled");
        assert!(request.get("enable_thinking").is_none());
    }

    #[test]
    fn common_think_tag_parser_handles_explicit_and_stray_close_tags() {
        let segments = split_common_think_tag_segments("<think>hidden</think>visible");
        assert_eq!(segments.len(), 2);
        assert!(matches!(&segments[0], CommonThinkSegment::Reasoning(text) if text == "hidden"));
        assert!(matches!(&segments[1], CommonThinkSegment::Text(text) if text == "visible"));

        let segments = split_common_think_tag_segments("hidden</think>visible");
        assert_eq!(segments.len(), 2);
        assert!(matches!(&segments[0], CommonThinkSegment::Reasoning(text) if text == "hidden"));
        assert!(matches!(&segments[1], CommonThinkSegment::Text(text) if text == "visible"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn claude_unary_sends_expected_headers_and_maps_tool_calls() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "claude-key"))
            .and(header("anthropic-version", CLAUDE_API_VERSION))
            .and(body_partial_json(json!({
                "model": "test-model",
                "system": "base prompt",
                "tools": [{
                    "name": "apply_patch"
                }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg_123",
                "content": [
                    { "type": "text", "text": "thinking" },
                    { "type": "tool_use", "id": "tool_1", "name": "apply_patch", "input": { "input": "*** Begin Patch\n*** End Patch\n" } }
                ],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 7,
                    "cache_read_input_tokens": 3
                }
            })))
            .mount(&server)
            .await;

        let prompt = Prompt {
            base_instructions: praxis_protocol::models::BaseInstructions {
                text: "base prompt".to_string(),
            },
            tools: vec![ToolSpec::Freeform(praxis_tools::FreeformTool {
                name: "apply_patch".to_string(),
                description: "Apply a patch".to_string(),
                format: praxis_tools::FreeformToolFormat {
                    r#type: "grammar".to_string(),
                    syntax: "lark".to_string(),
                    definition: "patch".to_string(),
                },
            })],
            ..Prompt::default()
        };

        let stream = stream_claude_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("claude-key"), None),
            &prompt,
            &model_info(),
        )
        .await
        .expect("claude stream");

        let events = drain_stream(stream).await;
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], ResponseEvent::Created));
        assert!(matches!(
            events[1],
            ResponseEvent::OutputItemDone(ResponseItem::Message { .. })
        ));
        assert!(
            matches!(events[2], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, .. }) if name == "apply_patch" && call_id == "tool_1")
        );
        assert!(
            matches!(events[3], ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 10, cached_input_tokens: 3, output_tokens: 7, .. }) } if response_id == "msg_123")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn common_unary_uses_chat_completions_and_maps_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer common-key"))
            .and(body_partial_json(json!({
                "model": "test-model",
                "stream": true
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl_1",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "done",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "local_shell",
                                "arguments": "{\"command\":[\"pwd\"]}"
                            }
                        }]
                    }
                }],
                "usage": {
                    "prompt_tokens": 21,
                    "completion_tokens": 9,
                    "total_tokens": 30,
                    "prompt_tokens_details": { "cached_tokens": 4 },
                    "completion_tokens_details": { "reasoning_tokens": 2 }
                }
            })))
            .mount(&server)
            .await;

        let stream = stream_common_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("common-key"), None),
            &common_provider_info(None),
            &Prompt::default(),
            &model_info(),
            None,
        )
        .await
        .expect("common stream");

        let events = drain_stream(stream).await;
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], ResponseEvent::Created));
        assert!(matches!(
            events[1],
            ResponseEvent::OutputItemDone(ResponseItem::Message { .. })
        ));
        assert!(
            matches!(events[2], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, ref arguments, .. }) if name == "local_shell" && call_id == "call_1" && arguments == "{\"command\":[\"pwd\"]}")
        );
        assert!(
            matches!(events[3], ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 21, cached_input_tokens: 4, cache_reported_input_tokens: 21, output_tokens: 9, reasoning_output_tokens: 2, total_tokens: 30 }) } if response_id == "chatcmpl_1")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn common_unary_maps_deepseek_prompt_cache_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer common-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl_deepseek_cache",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "done"
                    }
                }],
                "usage": {
                    "completion_tokens": 5,
                    "total_tokens": 25,
                    "prompt_cache_hit_tokens": 12,
                    "prompt_cache_miss_tokens": 8
                }
            })))
            .mount(&server)
            .await;

        let stream = stream_common_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("common-key"), None),
            &common_provider_info(None),
            &Prompt::default(),
            &model_info(),
            None,
        )
        .await
        .expect("common stream");

        let events = drain_stream(stream).await;
        assert!(matches!(events.last(), Some(ResponseEvent::Completed {
                response_id,
                token_usage: Some(TokenUsage {
                    input_tokens: 20,
                    cached_input_tokens: 12,
                    cache_reported_input_tokens: 20,
                    output_tokens: 5,
                    total_tokens: 25,
                    ..
                })
            }) if response_id == "chatcmpl_deepseek_cache"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn common_unary_preserves_reasoning_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer common-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl_reasoning",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "reasoning_content": "need a tool",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_reasoning",
                            "type": "function",
                            "function": {
                                "name": "local_shell",
                                "arguments": "{\"command\":[\"pwd\"]}"
                            }
                        }]
                    }
                }],
                "usage": {
                    "prompt_tokens": 1,
                    "completion_tokens": 1,
                    "total_tokens": 2
                }
            })))
            .mount(&server)
            .await;

        let stream = stream_common_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("common-key"), None),
            &common_provider_info(None),
            &Prompt::default(),
            &model_info(),
            None,
        )
        .await
        .expect("common stream");

        let events = drain_stream(stream).await;
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], ResponseEvent::Created));
        assert!(matches!(
            events[1],
            ResponseEvent::OutputItemDone(ResponseItem::Reasoning { ref content, .. })
                if matches!(
                    content.as_deref(),
                    Some([ReasoningItemContent::ReasoningText { text }]) if text == "need a tool"
                )
        ));
        assert!(
            matches!(events[2], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, .. }) if name == "local_shell" && call_id == "call_reasoning")
        );
        assert!(
            matches!(events[3], ResponseEvent::Completed { ref response_id, .. } if response_id == "chatcmpl_reasoning")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn common_unary_extracts_think_tags_from_message_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer common-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl_think_tags",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "hidden reasoning</think>visible answer"
                    }
                }],
                "usage": {
                    "prompt_tokens": 1,
                    "completion_tokens": 1,
                    "total_tokens": 2
                }
            })))
            .mount(&server)
            .await;

        let stream = stream_common_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("common-key"), None),
            &common_provider_info(None),
            &Prompt::default(),
            &model_info(),
            None,
        )
        .await
        .expect("common stream");

        let events = drain_stream(stream).await;
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], ResponseEvent::Created));
        assert!(matches!(
            events[1],
            ResponseEvent::OutputItemDone(ResponseItem::Reasoning { ref content, .. })
                if matches!(
                    content.as_deref(),
                    Some([ReasoningItemContent::ReasoningText { text }]) if text == "hidden reasoning"
                )
        ));
        assert!(matches!(
            events[2],
            ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
                if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "visible answer")
        ));
        assert!(
            matches!(events[3], ResponseEvent::Completed { ref response_id, .. } if response_id == "chatcmpl_think_tags")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn claude_sse_streams_text_then_tool_call() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "claude-key"))
            .and(body_partial_json(json!({
                "model": "test-model",
                "stream": true
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    [
                        sse_data(json!({
                            "type": "message_start",
                            "message": {
                                "id": "msg_stream",
                                "usage": {
                                    "input_tokens": 8,
                                    "cache_read_input_tokens": 2
                                }
                            }
                        })),
                        sse_data(json!({
                            "type": "content_block_start",
                            "index": 0,
                            "content_block": {
                                "type": "text",
                                "text": ""
                            }
                        })),
                        sse_data(json!({
                            "type": "content_block_delta",
                            "index": 0,
                            "delta": {
                                "type": "text_delta",
                                "text": "hel"
                            }
                        })),
                        sse_data(json!({
                            "type": "content_block_delta",
                            "index": 0,
                            "delta": {
                                "type": "text_delta",
                                "text": "lo"
                            }
                        })),
                        sse_data(json!({
                            "type": "content_block_start",
                            "index": 1,
                            "content_block": {
                                "type": "tool_use",
                                "id": "tool_stream",
                                "name": "apply_patch",
                                "input": {}
                            }
                        })),
                        sse_data(json!({
                            "type": "content_block_delta",
                            "index": 1,
                            "delta": {
                                "type": "input_json_delta",
                                "partial_json": "{\"input\":\"*** Begin Patch\\n*** End Patch\\n\"}"
                            }
                        })),
                        sse_data(json!({
                            "type": "content_block_stop",
                            "index": 1
                        })),
                        sse_data(json!({
                            "type": "message_delta",
                            "usage": {
                                "output_tokens": 5
                            }
                        })),
                        sse_data(json!({
                            "type": "message_stop"
                        })),
                    ]
                    .join(""),
                    "text/event-stream",
                ),
            )
            .mount(&server)
            .await;

        let stream = stream_claude_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("claude-key"), None),
            &Prompt::default(),
            &model_info(),
        )
        .await
        .expect("claude sse stream");

        let events = drain_stream(stream).await;
        assert_eq!(events.len(), 7);
        assert!(matches!(events[0], ResponseEvent::Created));
        assert!(matches!(
            events[1],
            ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
        ));
        assert!(matches!(events[2], ResponseEvent::OutputTextDelta(ref delta) if delta == "hel"));
        assert!(matches!(events[3], ResponseEvent::OutputTextDelta(ref delta) if delta == "lo"));
        assert!(matches!(
            events[4],
            ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
                if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "hello")
        ));
        assert!(matches!(
            events[5],
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, ref arguments, .. })
                if name == "apply_patch"
                    && call_id == "tool_stream"
                    && arguments == "{\"input\":\"*** Begin Patch\\n*** End Patch\\n\"}"
        ));
        assert!(matches!(
            events[6],
            ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 8, cached_input_tokens: 2, output_tokens: 5, total_tokens: 13, .. }) }
                if response_id == "msg_stream"
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn common_sse_streams_text_and_tool_call() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer common-key"))
            .and(body_partial_json(json!({
                "model": "test-model",
                "stream": true
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    [
                        sse_data(json!({
                            "id": "chat_stream_1",
                            "choices": [{
                                "delta": {
                                    "role": "assistant",
                                    "reasoning_content": "stream thought"
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_1",
                            "choices": [{
                                "delta": {
                                    "role": "assistant",
                                    "content": "hel"
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_1",
                            "choices": [{
                                "delta": {
                                    "content": "lo"
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_1",
                            "choices": [{
                                "delta": {
                                    "tool_calls": [{
                                        "index": 0,
                                        "id": "call_stream",
                                        "type": "function",
                                        "function": {
                                            "name": "local_shell",
                                            "arguments": "{\"command\":["
                                        }
                                    }]
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_1",
                            "choices": [{
                                "delta": {
                                    "tool_calls": [{
                                        "index": 0,
                                        "function": {
                                            "arguments": "\"pwd\"]}"
                                        }
                                    }]
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_1",
                            "choices": [{
                                "delta": {},
                                "finish_reason": "tool_calls"
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_1",
                            "choices": [],
                            "usage": {
                                "prompt_tokens": 12,
                                "completion_tokens": 4,
                                "total_tokens": 16,
                                "prompt_tokens_details": { "cached_tokens": 1 },
                                "completion_tokens_details": { "reasoning_tokens": 0 }
                            }
                        })),
                        "data: [DONE]\n\n".to_string(),
                    ]
                    .join(""),
                    "text/event-stream",
                ),
            )
            .mount(&server)
            .await;

        let stream = stream_common_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("common-key"), None),
            &common_provider_info(None),
            &Prompt::default(),
            &model_info(),
            None,
        )
        .await
        .expect("common sse stream");

        let events = drain_stream(stream).await;
        assert_eq!(events.len(), 10);
        assert!(matches!(events[0], ResponseEvent::Created));
        assert!(matches!(
            events[1],
            ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
        ));
        assert!(matches!(
            events[2],
            ResponseEvent::ReasoningContentDelta { ref delta, .. } if delta == "stream thought"
        ));
        assert!(matches!(
            events[3],
            ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. })
        ));
        assert!(matches!(
            events[4],
            ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
        ));
        assert!(matches!(events[5], ResponseEvent::OutputTextDelta(ref delta) if delta == "hel"));
        assert!(matches!(events[6], ResponseEvent::OutputTextDelta(ref delta) if delta == "lo"));
        assert!(matches!(
            events[7],
            ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
                if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "hello")
        ));
        assert!(matches!(
            events[8],
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, ref arguments, .. })
                if name == "local_shell"
                    && call_id == "call_stream"
                    && arguments == "{\"command\":[\"pwd\"]}"
        ));
        assert!(matches!(
            events[9],
            ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 12, cached_input_tokens: 1, output_tokens: 4, total_tokens: 16, .. }) }
                if response_id == "chat_stream_1"
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn common_sse_inferrs_spawn_agent_when_tool_name_is_empty() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer common-key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    [
                        sse_data(json!({
                            "id": "chat_stream_empty_tool_name",
                            "choices": [{
                                "delta": {
                                    "tool_calls": [{
                                        "index": 0,
                                        "id": "",
                                        "type": "function",
                                        "function": {
                                            "name": "",
                                            "arguments": "{\"task_name\":\"worker_a\",\"message\":\"do it\"}"
                                        }
                                    }]
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_empty_tool_name",
                            "choices": [{
                                "delta": {},
                                "finish_reason": "tool_calls"
                            }]
                        })),
                        "data: [DONE]\n\n".to_string(),
                    ]
                    .join(""),
                    "text/event-stream",
                ),
            )
            .mount(&server)
            .await;

        let stream = stream_common_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("common-key"), None),
            &common_provider_info(None),
            &Prompt::default(),
            &model_info(),
            None,
        )
        .await
        .expect("common sse stream");

        let events = drain_stream(stream).await;
        assert!(events.iter().any(|event| matches!(
            event,
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { name, call_id, arguments, .. })
                if name == "spawn_agent"
                    && call_id.starts_with("common-tool-0-")
                    && arguments == "{\"task_name\":\"worker_a\",\"message\":\"do it\"}"
        )));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn common_sse_extracts_think_tags_across_content_deltas() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer common-key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    [
                        sse_data(json!({
                            "id": "chat_stream_think",
                            "choices": [{
                                "delta": {
                                    "role": "assistant",
                                    "content": "<think>stream "
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_think",
                            "choices": [{
                                "delta": {
                                    "content": "thought</think>hel"
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_think",
                            "choices": [{
                                "delta": {
                                    "content": "lo"
                                },
                                "finish_reason": null
                            }]
                        })),
                        sse_data(json!({
                            "id": "chat_stream_think",
                            "choices": [{
                                "delta": {},
                                "finish_reason": "stop"
                            }]
                        })),
                        "data: [DONE]\n\n".to_string(),
                    ]
                    .join(""),
                    "text/event-stream",
                ),
            )
            .mount(&server)
            .await;

        let stream = stream_common_unary(
            provider(server.uri()),
            CoreAuthProvider::for_test(Some("common-key"), None),
            &common_provider_info(None),
            &Prompt::default(),
            &model_info(),
            None,
        )
        .await
        .expect("common sse stream");

        let events = drain_stream(stream).await;
        assert_eq!(events.len(), 8);
        assert!(matches!(events[0], ResponseEvent::Created));
        assert!(matches!(
            events[1],
            ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
        ));
        assert!(matches!(
            events[2],
            ResponseEvent::ReasoningContentDelta { ref delta, .. } if delta == "stream thought"
        ));
        assert!(matches!(
            events[3],
            ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. })
        ));
        assert!(matches!(
            events[4],
            ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
        ));
        assert!(matches!(
            events[5],
            ResponseEvent::OutputTextDelta(ref delta) if delta == "hello"
        ));
        assert!(matches!(
            events[6],
            ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
                if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "hello")
        ));
        assert!(
            matches!(events[7], ResponseEvent::Completed { ref response_id, .. } if response_id == "chat_stream_think")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "manual smoke test against a real Claude-compatible endpoint"]
    async fn manual_glm_claude_smoke() {
        let output_text =
            run_manual_glm_claude_prompt("Reply with exactly PONG and nothing else.").await;
        assert_eq!(output_text.trim(), "PONG");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "manual smoke test against a real Claude-compatible endpoint"]
    async fn manual_glm_claude_python_code_smoke() {
        let output_text = run_manual_glm_claude_prompt(
            "Write only Python code for a function `add_numbers(a, b)` that returns their sum. No explanation.",
        )
        .await;

        assert!(
            output_text.contains("def add_numbers"),
            "expected python function name in output: {output_text}"
        );
        assert!(
            output_text.contains("return"),
            "expected return statement in output: {output_text}"
        );
    }

    fn sse_data(value: Value) -> String {
        format!("data: {value}\n\n")
    }

    async fn drain_stream(mut stream: ResponseStream) -> Vec<ResponseEvent> {
        let mut events = Vec::new();
        while let Some(item) = stream.next().await {
            let event = item.expect("stream event");
            let is_completed = matches!(event, ResponseEvent::Completed { .. });
            events.push(event);
            if is_completed {
                break;
            }
        }
        events
    }

    async fn run_manual_glm_claude_prompt(user_text: &str) -> String {
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .expect("ANTHROPIC_BASE_URL must be set for manual GLM Claude tests");
        let model = std::env::var("ANTHROPIC_MODEL")
            .expect("ANTHROPIC_MODEL must be set for manual GLM Claude tests");
        let token = std::env::var("ANTHROPIC_AUTH_TOKEN")
            .expect("ANTHROPIC_AUTH_TOKEN must be set for manual GLM Claude tests");

        let prompt = Prompt {
            input: vec![ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: user_text.to_string(),
                }],
                end_turn: None,
                phase: None,
            }],
            ..Prompt::default()
        };

        let mut info = model_info();
        info.slug = model;

        let stream = stream_claude_unary(
            provider(base_url),
            CoreAuthProvider::for_test(Some(token.as_str()), None),
            &prompt,
            &info,
        )
        .await
        .expect("GLM Claude-compatible stream should succeed");

        let events = drain_stream(stream).await;
        assistant_output_text(&events)
    }

    fn assistant_output_text(events: &[ResponseEvent]) -> String {
        let deltas = events
            .iter()
            .filter_map(|event| match event {
                ResponseEvent::OutputTextDelta(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        if !deltas.is_empty() {
            return deltas;
        }

        events
            .iter()
            .filter_map(|event| match event {
                ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => {
                    Some(content)
                }
                _ => None,
            })
            .flat_map(|content| content.iter())
            .filter_map(|item| match item {
                ContentItem::OutputText { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

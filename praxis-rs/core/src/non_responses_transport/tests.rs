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

fn claude_provider_info(
    compat: Option<crate::model_provider_info::ModelProviderCompatInfo>,
) -> ModelProviderInfo {
    let mut provider = common_provider_info(compat);
    provider.name = "Claude Test Provider".to_string();
    provider.wire_api = crate::model_provider_info::WireApi::Claude;
    provider
}

fn gemini_provider_info() -> ModelProviderInfo {
    let mut provider = common_provider_info(None);
    provider.name = "Gemini".to_string();
    provider.base_url = Some(
        "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".to_string(),
    );
    provider
}

mod claude_sse;
mod claude_unary;
mod common_request_messages;
mod common_request_reasoning;
mod common_sse;
mod common_unary;
mod manual_smoke;

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
        &claude_provider_info(None),
        &prompt,
        &info,
        None,
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
            ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => Some(content),
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

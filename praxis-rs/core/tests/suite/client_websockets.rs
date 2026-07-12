#![allow(clippy::expect_used, clippy::unwrap_used)]
use core_test_support::load_default_config_for_test;
use core_test_support::responses::WebSocketConnectionConfig;
use core_test_support::responses::WebSocketTestServer;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::start_websocket_server;
use core_test_support::responses::start_websocket_server_with_headers;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::test_praxis;
use core_test_support::tracing::install_test_tracing;
use core_test_support::wait_for_event;
use futures::StreamExt;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use praxis_api::WS_REQUEST_HEADER_TRACEPARENT_CLIENT_METADATA_KEY;
use praxis_api::WS_REQUEST_HEADER_TRACESTATE_CLIENT_METADATA_KEY;
use praxis_core::ModelClient;
use praxis_core::ModelClientSession;
use praxis_core::ModelProviderInfo;
use praxis_core::Prompt;
use praxis_core::ResponseEvent;
use praxis_core::WireApi;
use praxis_core::X_RESPONSESAPI_INCLUDE_TIMING_METRICS_HEADER;
use praxis_features::Feature;
use praxis_login::OpenAiAccountAuth;
use praxis_otel::SessionTelemetry;
use praxis_otel::TelemetryAuthMode;
use praxis_otel::current_span_w3c_trace_context;
use praxis_otel::metrics::MetricsClient;
use praxis_otel::metrics::MetricsConfig;
use praxis_protocol::ThreadId;
use praxis_protocol::account::PlanType;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::W3cTraceContext;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tracing::Instrument;
use tracing_test::traced_test;

const MODEL: &str = "gpt-5.2-codex";
const OPENAI_BETA_HEADER: &str = "OpenAI-Beta";
const WS_V2_BETA_HEADER_VALUE: &str = "responses_websockets=2026-02-06";
const X_CLIENT_REQUEST_ID_HEADER: &str = "x-client-request-id";

fn assert_request_trace_matches(body: &serde_json::Value, expected_trace: &W3cTraceContext) {
    let client_metadata = body["client_metadata"]
        .as_object()
        .expect("missing client_metadata payload");
    let actual_traceparent = client_metadata
        .get(WS_REQUEST_HEADER_TRACEPARENT_CLIENT_METADATA_KEY)
        .and_then(serde_json::Value::as_str)
        .expect("missing traceparent");
    let expected_traceparent = expected_trace
        .traceparent
        .as_deref()
        .expect("missing expected traceparent");

    assert_eq!(actual_traceparent, expected_traceparent);
    assert_eq!(
        client_metadata
            .get(WS_REQUEST_HEADER_TRACESTATE_CLIENT_METADATA_KEY)
            .and_then(serde_json::Value::as_str),
        expected_trace.tracestate.as_deref()
    );
    assert!(
        body.get("trace").is_none(),
        "top-level trace should not be sent"
    );
}

struct WebsocketTestHarness {
    _praxis_home: TempDir,
    client: ModelClient,
    conversation_id: ThreadId,
    model_info: ModelInfo,
    effort: Option<ReasoningEffortConfig>,
    summary: ReasoningSummary,
    session_telemetry: SessionTelemetry,
}

fn message_item(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".into(),
        content: vec![ContentItem::InputText { text: text.into() }],
        end_turn: None,
        phase: None,
    }
}

fn assistant_message_item(id: &str, text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: Some(id.to_string()),
        role: "assistant".into(),
        content: vec![ContentItem::OutputText { text: text.into() }],
        end_turn: None,
        phase: None,
    }
}

fn prompt_with_input(input: Vec<ResponseItem>) -> Prompt {
    let mut prompt = Prompt::default();
    prompt.input = input;
    prompt
}

fn prompt_with_input_and_instructions(input: Vec<ResponseItem>, instructions: &str) -> Prompt {
    let mut prompt = prompt_with_input(input);
    prompt.base_instructions = BaseInstructions {
        text: instructions.to_string(),
    };
    prompt
}

fn websocket_provider(server: &WebSocketTestServer) -> ModelProviderInfo {
    websocket_provider_with_connect_timeout(server, /*websocket_connect_timeout_ms*/ None)
}

fn websocket_provider_with_connect_timeout(
    server: &WebSocketTestServer,
    websocket_connect_timeout_ms: Option<u64>,
) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "mock-ws".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        compat: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        websocket_connect_timeout_ms,
        requires_openai_auth: false,
        supports_websockets: true,
    }
}

async fn websocket_harness(server: &WebSocketTestServer) -> WebsocketTestHarness {
    websocket_harness_with_runtime_metrics(server, /*runtime_metrics_enabled*/ false).await
}

async fn websocket_harness_with_runtime_metrics(
    server: &WebSocketTestServer,
    runtime_metrics_enabled: bool,
) -> WebsocketTestHarness {
    websocket_harness_with_options(server, runtime_metrics_enabled).await
}

async fn websocket_harness_with_v2(
    server: &WebSocketTestServer,
    runtime_metrics_enabled: bool,
) -> WebsocketTestHarness {
    websocket_harness_with_options(server, runtime_metrics_enabled).await
}

async fn websocket_harness_with_options(
    server: &WebSocketTestServer,
    runtime_metrics_enabled: bool,
) -> WebsocketTestHarness {
    websocket_harness_with_provider_options(websocket_provider(server), runtime_metrics_enabled)
        .await
}

async fn websocket_harness_with_provider_options(
    provider: ModelProviderInfo,
    runtime_metrics_enabled: bool,
) -> WebsocketTestHarness {
    let praxis_home = TempDir::new().unwrap();
    let mut config = load_default_config_for_test(&praxis_home).await;
    config.model = Some(MODEL.to_string());
    if runtime_metrics_enabled {
        config
            .features
            .enable(Feature::RuntimeMetrics)
            .expect("test config should allow feature update");
    }
    let config = Arc::new(config);
    let model_info = praxis_core::test_support::construct_model_info_offline(MODEL, &config);
    let conversation_id = ThreadId::new();
    let auth_manager = praxis_core::test_support::auth_manager_from_auth(
        OpenAiAccountAuth::from_api_key("Test API Key"),
    );
    let exporter = InMemoryMetricExporter::default();
    let metrics = MetricsClient::new(
        MetricsConfig::in_memory("test", "praxis-core", env!("CARGO_PKG_VERSION"), exporter)
            .with_runtime_reader(),
    )
    .expect("in-memory metrics client");
    let session_telemetry = SessionTelemetry::new(
        conversation_id,
        MODEL,
        model_info.slug.as_str(),
        /*account_id*/ None,
        Some("test@test.com".to_string()),
        auth_manager.auth_mode().map(TelemetryAuthMode::from),
        "test_originator".to_string(),
        /*log_user_prompts*/ false,
        "test".to_string(),
        SessionSource::Exec,
    )
    .with_metrics(metrics);
    let effort = None;
    let summary = ReasoningSummary::Auto;
    let client = ModelClient::new(
        /*auth_manager*/ None,
        conversation_id,
        "test-provider".to_string(),
        provider.clone(),
        SessionSource::Exec,
        config.model_verbosity,
        /*enable_request_compression*/ false,
        runtime_metrics_enabled,
        /*beta_features_header*/ None,
    );

    WebsocketTestHarness {
        _praxis_home: praxis_home,
        client,
        conversation_id,
        model_info,
        effort,
        summary,
        session_telemetry,
    }
}

async fn stream_until_complete(
    client_session: &mut ModelClientSession,
    harness: &WebsocketTestHarness,
    prompt: &Prompt,
) {
    stream_until_complete_with_service_tier(
        client_session,
        harness,
        prompt,
        /*service_tier*/ None,
    )
    .await;
}

async fn stream_until_complete_with_service_tier(
    client_session: &mut ModelClientSession,
    harness: &WebsocketTestHarness,
    prompt: &Prompt,
    service_tier: Option<ServiceTier>,
) {
    stream_until_complete_with_turn_metadata(
        client_session,
        harness,
        prompt,
        service_tier,
        /*turn_metadata_header*/ None,
    )
    .await;
}

async fn stream_until_complete_with_turn_metadata(
    client_session: &mut ModelClientSession,
    harness: &WebsocketTestHarness,
    prompt: &Prompt,
    service_tier: Option<ServiceTier>,
    turn_metadata_header: Option<&str>,
) {
    let mut stream = client_session
        .stream(
            prompt,
            &harness.model_info,
            &harness.session_telemetry,
            harness.effort,
            harness.summary,
            service_tier,
            turn_metadata_header,
        )
        .await
        .expect("websocket stream failed");

    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }
}

#[path = "client_websockets/connection_reuse.rs"]
mod connection_reuse;
#[path = "client_websockets/errors_and_incremental.rs"]
mod errors_and_incremental;
#[path = "client_websockets/telemetry_headers_and_limits.rs"]
mod telemetry_headers_and_limits;
#[path = "client_websockets/v2_and_prewarm.rs"]
mod v2_and_prewarm;
#[path = "client_websockets/v2_errors_and_headers.rs"]
mod v2_errors_and_headers;

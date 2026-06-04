use crate::auth::AuthProvider;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use http::HeaderMap;
use http::Method;
use http::header::ETAG;
use praxis_client::HttpTransport;
use praxis_client::RequestTelemetry;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ModelsResponse;
#[cfg(test)]
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::openai_models::WebSearchToolType;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use praxis_protocol::openai_models::provider_neutral_reasoning_levels;
use serde::Deserialize;
use std::sync::Arc;

pub struct ModelsClient<T: HttpTransport, A: AuthProvider> {
    session: EndpointSession<T, A>,
}

impl<T: HttpTransport, A: AuthProvider> ModelsClient<T, A> {
    pub fn new(transport: T, provider: Provider, auth: A) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
        }
    }

    pub fn with_telemetry(self, request: Option<Arc<dyn RequestTelemetry>>) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
        }
    }

    fn path() -> &'static str {
        "models"
    }

    fn append_client_version_query(req: &mut praxis_client::Request, client_version: &str) {
        let separator = if req.url.contains('?') { '&' } else { '?' };
        req.url = format!("{}{}client_version={client_version}", req.url, separator);
    }

    pub async fn list_models(
        &self,
        client_version: &str,
        extra_headers: HeaderMap,
    ) -> Result<(Vec<ModelInfo>, Option<String>), ApiError> {
        let resp = self
            .session
            .execute_with(
                Method::GET,
                Self::path(),
                extra_headers,
                /*body*/ None,
                |req| {
                    Self::append_client_version_query(req, client_version);
                },
            )
            .await?;

        let header_etag = resp
            .headers
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string);

        let models = decode_models_response(resp.body.as_ref()).map_err(|e| {
            ApiError::Stream(format!(
                "failed to decode models response: {e}; body: {}",
                String::from_utf8_lossy(&resp.body)
            ))
        })?;

        Ok((models, header_etag))
    }
}

fn decode_models_response(body: &[u8]) -> serde_json::Result<Vec<ModelInfo>> {
    match serde_json::from_slice::<ModelsResponse>(body) {
        Ok(ModelsResponse { models }) => Ok(models),
        Err(full_error) => match serde_json::from_slice::<OpenAiModelsResponse>(body) {
            Ok(response) => Ok(response
                .data
                .into_iter()
                .map(|model| openai_model_to_model_info(&model.id))
                .collect()),
            Err(_) => Err(full_error),
        },
    }
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModelEntry>,
}

#[derive(Deserialize)]
struct OpenAiModelEntry {
    id: String,
}

fn openai_model_to_model_info(model_id: &str) -> ModelInfo {
    if let Some(model) = known_openai_compatible_model_info(model_id) {
        return model;
    }

    let (default_reasoning_level, supported_reasoning_levels) = provider_neutral_reasoning_levels();
    ModelInfo {
        slug: model_id.to_string(),
        display_name: model_id.to_string(),
        description: None,
        default_reasoning_level,
        supported_reasoning_levels,
        shell_type: ConfigShellToolType::Default,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority: 99,
        availability_nux: None,
        upgrade: None,
        base_instructions: praxis_protocol::models::BASE_INSTRUCTIONS_DEFAULT.to_string(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None,
        apply_patch_tool_type: None,
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: Some(272_000),
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
        input_modalities: default_input_modalities(),
        used_fallback_model_metadata: false,
        supports_search_tool: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::RetryConfig;
    use async_trait::async_trait;
    use http::HeaderMap;
    use http::StatusCode;
    use praxis_client::Request;
    use praxis_client::Response;
    use praxis_client::StreamResponse;
    use praxis_client::TransportError;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Clone)]
    struct CapturingTransport {
        last_request: Arc<Mutex<Option<Request>>>,
        body: Arc<ModelsResponse>,
        etag: Option<String>,
    }

    impl Default for CapturingTransport {
        fn default() -> Self {
            Self {
                last_request: Arc::new(Mutex::new(None)),
                body: Arc::new(ModelsResponse { models: Vec::new() }),
                etag: None,
            }
        }
    }

    #[async_trait]
    impl HttpTransport for CapturingTransport {
        async fn execute(&self, req: Request) -> Result<Response, TransportError> {
            *self.last_request.lock().unwrap() = Some(req);
            let body = serde_json::to_vec(&*self.body).unwrap();
            let mut headers = HeaderMap::new();
            if let Some(etag) = &self.etag {
                headers.insert(ETAG, etag.parse().unwrap());
            }
            Ok(Response {
                status: StatusCode::OK,
                headers,
                body: body.into(),
            })
        }

        async fn stream(&self, _req: Request) -> Result<StreamResponse, TransportError> {
            Err(TransportError::Build("stream should not run".to_string()))
        }
    }

    #[derive(Clone, Default)]
    struct DummyAuth;

    impl AuthProvider for DummyAuth {
        fn bearer_token(&self) -> Option<String> {
            None
        }
    }

    fn provider(base_url: &str) -> Provider {
        Provider {
            name: "test".to_string(),
            base_url: base_url.to_string(),
            query_params: None,
            headers: HeaderMap::new(),
            retry: RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                retry_429: false,
                retry_5xx: true,
                retry_transport: true,
            },
            stream_idle_timeout: Duration::from_secs(1),
        }
    }

    #[tokio::test]
    async fn appends_client_version_query() {
        let response = ModelsResponse { models: Vec::new() };

        let transport = CapturingTransport {
            last_request: Arc::new(Mutex::new(None)),
            body: Arc::new(response),
            etag: None,
        };

        let client = ModelsClient::new(
            transport.clone(),
            provider("https://example.com/api/codex"),
            DummyAuth,
        );

        let (models, _) = client
            .list_models("0.99.0", HeaderMap::new())
            .await
            .expect("request should succeed");

        assert_eq!(models.len(), 0);

        let url = transport
            .last_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .url
            .clone();
        assert_eq!(
            url,
            "https://example.com/api/codex/models?client_version=0.99.0"
        );
    }

    #[tokio::test]
    async fn parses_models_response() {
        let response = ModelsResponse {
            models: vec![
                serde_json::from_value(json!({
                    "slug": "gpt-test",
                    "display_name": "gpt-test",
                    "description": "desc",
                    "default_reasoning_level": "medium",
                    "supported_reasoning_levels": [{"effort": "low", "description": "low"}, {"effort": "medium", "description": "medium"}, {"effort": "high", "description": "high"}],
                    "shell_type": "shell_command",
                    "visibility": "list",
                    "minimal_client_version": [0, 99, 0],
                    "supported_in_api": true,
                    "priority": 1,
                    "upgrade": null,
                    "base_instructions": "base instructions",
                    "supports_reasoning_summaries": false,
                    "support_verbosity": false,
                    "default_verbosity": null,
                    "apply_patch_tool_type": null,
                    "truncation_policy": {"mode": "bytes", "limit": 10_000},
                    "supports_parallel_tool_calls": false,
                    "supports_image_detail_original": false,
                    "context_window": 272_000,
                    "experimental_supported_tools": [],
                }))
                .unwrap(),
            ],
        };

        let transport = CapturingTransport {
            last_request: Arc::new(Mutex::new(None)),
            body: Arc::new(response),
            etag: None,
        };

        let client = ModelsClient::new(
            transport,
            provider("https://example.com/api/codex"),
            DummyAuth,
        );

        let (models, _) = client
            .list_models("0.99.0", HeaderMap::new())
            .await
            .expect("request should succeed");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].slug, "gpt-test");
        assert_eq!(models[0].supported_in_api, true);
        assert_eq!(models[0].priority, 1);
    }

    #[tokio::test]
    async fn list_models_includes_etag() {
        let response = ModelsResponse { models: Vec::new() };

        let transport = CapturingTransport {
            last_request: Arc::new(Mutex::new(None)),
            body: Arc::new(response),
            etag: Some("\"abc\"".to_string()),
        };

        let client = ModelsClient::new(
            transport,
            provider("https://example.com/api/codex"),
            DummyAuth,
        );

        let (models, etag) = client
            .list_models("0.1.0", HeaderMap::new())
            .await
            .expect("request should succeed");

        assert_eq!(models.len(), 0);
        assert_eq!(etag, Some("\"abc\"".to_string()));
    }

    #[test]
    fn parses_openai_compatible_models_response() {
        let models = decode_models_response(
            br#"{"object":"list","data":[{"id":"deepseek-v4-flash","object":"model","owned_by":"deepseek"}]}"#,
        )
        .expect("OpenAI-compatible model list should decode");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].slug, "deepseek-v4-flash");
        assert_eq!(models[0].display_name, "DeepSeek V4 Flash");
        assert_eq!(models[0].visibility, ModelVisibility::List);
        assert!(models[0].supported_in_api);
        assert_eq!(
            models[0].default_reasoning_level,
            Some(ReasoningEffort::High)
        );
        assert_eq!(models[0].context_window, Some(1_000_000));
        assert_eq!(models[0].auto_compact_token_limit(), Some(900_000));
        assert_eq!(
            models[0]
                .supported_reasoning_levels
                .iter()
                .map(|preset| preset.effort)
                .collect::<Vec<_>>(),
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
                ReasoningEffort::None
            ]
        );
    }

    #[test]
    fn parses_openai_compatible_models_with_common_reasoning_levels() {
        let models = decode_models_response(
            br#"{"object":"list","data":[{"id":"custom-reasoning-model","object":"model","owned_by":"custom"}]}"#,
        )
        .expect("OpenAI-compatible model list should decode");

        assert_eq!(models[0].slug, "custom-reasoning-model");
        assert_eq!(models[0].default_reasoning_level, None);
        assert_eq!(
            models[0]
                .supported_reasoning_levels
                .iter()
                .map(|preset| preset.effort)
                .collect::<Vec<_>>(),
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
                ReasoningEffort::None
            ]
        );
    }
}

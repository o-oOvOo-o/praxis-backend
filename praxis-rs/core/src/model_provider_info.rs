//! Registry of model providers supported by Praxis.
//!
//! Providers can be defined in two places:
//!   1. Built-in defaults compiled into the binary so Praxis works out-of-the-box.
//!   2. User-defined entries inside `~/.praxis/config.toml` under the `model_providers`
//!      key. These override or extend the defaults at runtime.

use praxis_protocol::config_types::ModelProviderAuthInfo;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_STREAM_MAX_RETRIES: u64 = 5;
const DEFAULT_REQUEST_MAX_RETRIES: u64 = 4;
pub(crate) const DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS: u64 = 15_000;
/// Hard cap for user-configured `stream_max_retries`.
const MAX_STREAM_MAX_RETRIES: u64 = 100;
/// Hard cap for user-configured `request_max_retries`.
const MAX_REQUEST_MAX_RETRIES: u64 = 100;

const OPENAI_PROVIDER_NAME: &str = "OpenAI";
pub const OPENAI_PROVIDER_ID: &str = "openai";
const ANTHROPIC_PROVIDER_NAME: &str = "Anthropic";
pub const ANTHROPIC_PROVIDER_ID: &str = "anthropic";
pub const ANTHROPIC_API_KEY_ENV_VAR: &str = "ANTHROPIC_API_KEY";
pub const ANTHROPIC_API_BASE_URL: &str = "https://api.anthropic.com";
pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const CHAT_WIRE_API_REMOVED_ERROR: &str = "`wire_api = \"chat\"` is no longer supported.\nHow to fix: set `wire_api = \"responses\"` in your provider config.";
pub(crate) const LEGACY_OLLAMA_CHAT_PROVIDER_ID: &str = "ollama-chat";
pub(crate) const OLLAMA_CHAT_PROVIDER_REMOVED_ERROR: &str = "`ollama-chat` is no longer supported.\nHow to fix: replace `ollama-chat` with `ollama` in `model_provider`, `oss_provider`, or `--local-provider`.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirstPartyModelOwner {
    pub provider_id: &'static str,
    pub owner_label: &'static str,
}

pub fn first_party_model_owner(model: &str) -> Option<FirstPartyModelOwner> {
    let policy = crate::llm::registry::LlmProfileRegistry::builtin_static()
        .first_party_policy_for_model(model)?;
    Some(FirstPartyModelOwner {
        provider_id: policy.canonical_provider_id()?,
        owner_label: policy.owner_label(),
    })
}

pub fn provider_accepts_registered_model_catalog(
    provider_id: &str,
    provider: &ModelProviderInfo,
    model: &str,
) -> bool {
    if is_native_local_provider(provider_id, provider) {
        return true;
    }
    crate::llm::registry::LlmProfileRegistry::builtin_static()
        .provider_accepts_known_first_party_model(provider_id, provider, model)
}

pub fn provider_accepts_known_first_party_model(
    provider_id: &str,
    provider: &ModelProviderInfo,
    model: &str,
) -> bool {
    provider_accepts_registered_model_catalog(provider_id, provider, model)
}

/// Wire protocol that the provider speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    /// The Responses API exposed by OpenAI at `/v1/responses`.
    #[default]
    Responses,
    /// Anthropic/Claude-style messages API.
    Claude,
    /// Generic OpenAI-compatible chat/completions-style API.
    #[serde(rename = "openai_compat", alias = "common")]
    OpenAiCompat,
}

impl fmt::Display for WireApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Responses => "responses",
            Self::Claude => "claude",
            Self::OpenAiCompat => "openai_compat",
        };
        f.write_str(value)
    }
}

impl<'de> Deserialize<'de> for WireApi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "responses" => Ok(Self::Responses),
            "claude" => Ok(Self::Claude),
            "openai_compat" | "common" => Ok(Self::OpenAiCompat),
            "chat" => Err(serde::de::Error::custom(CHAT_WIRE_API_REMOVED_ERROR)),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["responses", "claude", "openai_compat"],
            )),
        }
    }
}

/// Provider-specific compatibility shims for non-standard implementations that
/// share a broader wire API family.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderCompatInfo {
    /// Whether developer-role messages can be sent distinctly instead of being
    /// folded into a system prompt.
    pub supports_developer_role: Option<bool>,

    /// Whether the provider accepts explicit reasoning effort controls.
    pub supports_reasoning_effort: Option<bool>,

    /// Optional mapping from Praxis effort labels to provider-specific values.
    pub reasoning_effort_map: Option<ModelProviderReasoningEffortMap>,

    /// Whether the provider accepts the `parallel_tool_calls` request field.
    pub supports_parallel_tool_calls: Option<bool>,

    /// Which field name the provider expects for output token limits.
    pub max_tokens_field: Option<ModelProviderMaxTokensField>,

    /// Optional output token cap to use with `max_tokens_field`.
    pub max_tokens: Option<i64>,

    /// Whether tool-result messages must include the tool name.
    pub requires_tool_result_name: Option<bool>,

    /// Whether an assistant bridge message is required before a user message
    /// immediately following tool results.
    pub requires_assistant_after_tool_result: Option<bool>,

    /// Optional provider-specific reasoning/thinking payload style.
    pub thinking_format: Option<ModelProviderThinkingFormat>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderReasoningEffortMap {
    pub minimal: Option<String>,
    pub low: Option<String>,
    pub medium: Option<String>,
    pub high: Option<String>,
    pub xhigh: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderMaxTokensField {
    MaxCompletionTokens,
    MaxTokens,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderThinkingFormat {
    Openai,
    Openrouter,
    Deepseek,
    Gemini,
    Zai,
    Qwen,
    QwenChatTemplate,
    #[serde(alias = "llama_cpp_chat_template")]
    ChatTemplateKwargs,
}

/// Serializable representation of a provider definition.
#[derive(Clone, Deserialize, Serialize, PartialEq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderInfo {
    /// Friendly display name.
    pub name: String,
    /// Base URL for the provider's OpenAI-compatible API.
    pub base_url: Option<String>,
    /// Environment variable that stores the user's API key for this provider.
    pub env_key: Option<String>,

    /// Optional instructions to help the user get a valid value for the
    /// variable and set it.
    pub env_key_instructions: Option<String>,

    /// Value to use with `Authorization: Bearer <token>` header. Use of this
    /// config is discouraged in favor of `env_key` for security reasons, but
    /// this may be necessary when using this programmatically.
    pub experimental_bearer_token: Option<String>,

    /// Command-backed bearer-token configuration for this provider.
    pub auth: Option<ModelProviderAuthInfo>,

    /// Which wire protocol this provider expects.
    #[serde(default)]
    pub wire_api: WireApi,

    /// Optional compatibility shims for provider-specific quirks inside the
    /// selected wire API family.
    pub compat: Option<ModelProviderCompatInfo>,

    /// Optional query parameters to append to the base URL.
    pub query_params: Option<HashMap<String, String>>,

    /// Additional HTTP headers to include in requests to this provider where
    /// the (key, value) pairs are the header name and value.
    pub http_headers: Option<HashMap<String, String>>,

    /// Optional HTTP headers to include in requests to this provider where the
    /// (key, value) pairs are the header name and _environment variable_ whose
    /// value should be used. If the environment variable is not set, or the
    /// value is empty, the header will not be included in the request.
    pub env_http_headers: Option<HashMap<String, String>>,

    /// Maximum number of times to retry a failed HTTP request to this provider.
    pub request_max_retries: Option<u64>,

    /// Number of times to retry reconnecting a dropped streaming response before failing.
    pub stream_max_retries: Option<u64>,

    /// Idle timeout (in milliseconds) to wait for activity on a streaming response before treating
    /// the connection as lost.
    pub stream_idle_timeout_ms: Option<u64>,

    /// Maximum time (in milliseconds) to wait for a websocket connection attempt before treating
    /// it as failed.
    pub websocket_connect_timeout_ms: Option<u64>,

    /// Does this provider require an OpenAI API Key or ChatGPT login token? If true,
    /// user is presented with login screen on first run, and login preference and token/key
    /// are stored in auth.json. If false (which is the default), login screen is skipped,
    /// and API key (if needed) comes from the "env_key" environment variable.
    #[serde(default)]
    pub requires_openai_auth: bool,

    /// Whether this provider supports the Responses API WebSocket transport.
    #[serde(default)]
    pub supports_websockets: bool,
}

impl fmt::Debug for ModelProviderInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base_url = self.base_url.as_deref().map(redacted_provider_url);
        formatter
            .debug_struct("ModelProviderInfo")
            .field("name", &self.name)
            .field("base_url", &base_url)
            .field("env_key", &self.env_key)
            .field(
                "env_key_instructions",
                &self.env_key_instructions.as_ref().map(|_| "[CONFIGURED]"),
            )
            .field(
                "experimental_bearer_token",
                &self
                    .experimental_bearer_token
                    .as_ref()
                    .map(|_| "[REDACTED]"),
            )
            .field("auth", &self.auth.as_ref().map(|_| "[CONFIGURED]"))
            .field("wire_api", &self.wire_api)
            .field("compat", &self.compat)
            .field(
                "query_param_names",
                &sorted_map_keys(self.query_params.as_ref()),
            )
            .field(
                "http_header_names",
                &sorted_map_keys(self.http_headers.as_ref()),
            )
            .field(
                "env_http_header_names",
                &sorted_map_keys(self.env_http_headers.as_ref()),
            )
            .field("request_max_retries", &self.request_max_retries)
            .field("stream_max_retries", &self.stream_max_retries)
            .field("stream_idle_timeout_ms", &self.stream_idle_timeout_ms)
            .field(
                "websocket_connect_timeout_ms",
                &self.websocket_connect_timeout_ms,
            )
            .field("requires_openai_auth", &self.requires_openai_auth)
            .field("supports_websockets", &self.supports_websockets)
            .finish()
    }
}

fn sorted_map_keys(map: Option<&HashMap<String, String>>) -> Vec<&str> {
    let mut keys: Vec<&str> = map
        .map(|map| map.keys().map(String::as_str).collect())
        .unwrap_or_default();
    keys.sort_unstable();
    keys
}

fn redacted_provider_url(raw: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw) else {
        return "[UNPARSEABLE URL]".to_string();
    };
    if !url.username().is_empty() {
        let _ = url.set_username("[REDACTED]");
    }
    if url.password().is_some() {
        let _ = url.set_password(Some("[REDACTED]"));
    }
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

impl ModelProviderInfo {
    pub(crate) fn validate(&self) -> std::result::Result<(), String> {
        let Some(auth) = self.auth.as_ref() else {
            return Ok(());
        };

        if auth.command.trim().is_empty() {
            return Err("provider auth.command must not be empty".to_string());
        }

        let mut conflicts = Vec::new();
        if self.env_key.is_some() {
            conflicts.push("env_key");
        }
        if self.experimental_bearer_token.is_some() {
            conflicts.push("experimental_bearer_token");
        }
        if self.requires_openai_auth {
            conflicts.push("requires_openai_auth");
        }

        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "provider auth cannot be combined with {}",
                conflicts.join(", ")
            ))
        }
    }

    /// Effective maximum number of request retries for this provider.
    pub fn request_max_retries(&self) -> u64 {
        self.request_max_retries
            .unwrap_or(DEFAULT_REQUEST_MAX_RETRIES)
            .min(MAX_REQUEST_MAX_RETRIES)
    }

    /// Effective maximum number of stream reconnection attempts for this provider.
    pub fn stream_max_retries(&self) -> u64 {
        self.stream_max_retries
            .unwrap_or(DEFAULT_STREAM_MAX_RETRIES)
            .min(MAX_STREAM_MAX_RETRIES)
    }

    /// Effective idle timeout for streaming responses.
    pub fn stream_idle_timeout(&self) -> Duration {
        self.stream_idle_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(DEFAULT_STREAM_IDLE_TIMEOUT_MS))
    }

    /// Effective timeout for websocket connect attempts.
    pub fn websocket_connect_timeout(&self) -> Duration {
        self.websocket_connect_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS))
    }

    pub fn create_openai_provider(base_url: Option<String>) -> ModelProviderInfo {
        ModelProviderInfo {
            name: OPENAI_PROVIDER_NAME.into(),
            base_url,
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::Responses,
            compat: None,
            query_params: None,
            http_headers: Some(
                [("version".to_string(), env!("CARGO_PKG_VERSION").to_string())]
                    .into_iter()
                    .collect(),
            ),
            env_http_headers: Some(
                [
                    (
                        "OpenAI-Organization".to_string(),
                        "OPENAI_ORGANIZATION".to_string(),
                    ),
                    ("OpenAI-Project".to_string(), "OPENAI_PROJECT".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            // Use global defaults for retry/timeout unless overridden in config.toml.
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            websocket_connect_timeout_ms: None,
            requires_openai_auth: true,
            supports_websockets: true,
        }
    }

    pub fn create_anthropic_provider() -> ModelProviderInfo {
        ModelProviderInfo {
            name: ANTHROPIC_PROVIDER_NAME.into(),
            base_url: Some(ANTHROPIC_API_BASE_URL.into()),
            env_key: Some(ANTHROPIC_API_KEY_ENV_VAR.into()),
            env_key_instructions: Some(
                "Run `/login anthropic` to use the local Claude Pro/Max OAuth login, or provide a Claude Console API key with ANTHROPIC_API_KEY."
                    .into(),
            ),
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::Claude,
            compat: Some(ModelProviderCompatInfo {
                supports_reasoning_effort: Some(true),
                max_tokens: Some(64 * 1024),
                ..Default::default()
            }),
            query_params: None,
            http_headers: Some(
                [(
                    "anthropic-version".to_string(),
                    ANTHROPIC_API_VERSION.to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            websocket_connect_timeout_ms: None,
            requires_openai_auth: false,
            supports_websockets: false,
        }
    }

    pub fn is_openai(&self) -> bool {
        self.name == OPENAI_PROVIDER_NAME
    }

    pub fn is_anthropic(&self) -> bool {
        self.name == ANTHROPIC_PROVIDER_NAME
            && self.wire_api == WireApi::Claude
            && self.base_url.as_deref() == Some(ANTHROPIC_API_BASE_URL)
    }

    pub(crate) fn uses_managed_openai_auth(&self) -> bool {
        self.requires_openai_auth || self.is_openai()
    }

    pub(crate) fn can_use_managed_auth(&self) -> bool {
        self.has_command_auth() || self.uses_managed_openai_auth()
    }

    pub(crate) fn has_command_auth(&self) -> bool {
        self.auth.is_some()
    }
}

pub const DEFAULT_LMSTUDIO_PORT: u16 = 1234;
pub const DEFAULT_OLLAMA_PORT: u16 = 11434;

pub const LMSTUDIO_OSS_PROVIDER_ID: &str = "lmstudio";
pub const NATIVE_LOCAL_PROVIDER_ID: &str = "praxis-native-local";
pub const OLLAMA_OSS_PROVIDER_ID: &str = "ollama";

const PRAXIS_OSS_PORT_ENV: &str = "PRAXIS_OSS_PORT";
const PRAXIS_OSS_BASE_URL_ENV: &str = "PRAXIS_OSS_BASE_URL";

/// Built-in default provider list.
pub fn built_in_model_providers(
    openai_base_url: Option<String>,
) -> HashMap<String, ModelProviderInfo> {
    use ModelProviderInfo as P;
    let openai_provider = P::create_openai_provider(openai_base_url);
    let anthropic_provider = P::create_anthropic_provider();

    [
        (OPENAI_PROVIDER_ID, openai_provider),
        (ANTHROPIC_PROVIDER_ID, anthropic_provider),
        (
            OLLAMA_OSS_PROVIDER_ID,
            create_oss_provider(DEFAULT_OLLAMA_PORT, WireApi::Responses),
        ),
        (
            LMSTUDIO_OSS_PROVIDER_ID,
            create_oss_provider(DEFAULT_LMSTUDIO_PORT, WireApi::Responses),
        ),
        (NATIVE_LOCAL_PROVIDER_ID, create_native_local_provider()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

pub fn is_native_local_provider(provider_id: &str, provider: &ModelProviderInfo) -> bool {
    provider_id == NATIVE_LOCAL_PROVIDER_ID
        || provider.base_url.as_deref() == Some("praxis-native://local")
}

pub fn create_native_local_provider() -> ModelProviderInfo {
    ModelProviderInfo {
        name: "Praxis Native Local".into(),
        base_url: Some("praxis-native://local".into()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::OpenAiCompat,
        compat: None,
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

pub fn create_oss_provider(default_provider_port: u16, wire_api: WireApi) -> ModelProviderInfo {
    let default_praxis_oss_base_url = format!(
        "http://localhost:{praxis_oss_port}/v1",
        praxis_oss_port = provider_env_value(PRAXIS_OSS_PORT_ENV)
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(default_provider_port)
    );

    let praxis_oss_base_url =
        provider_env_value(PRAXIS_OSS_BASE_URL_ENV).unwrap_or(default_praxis_oss_base_url);
    create_oss_provider_with_base_url(&praxis_oss_base_url, wire_api)
}

fn provider_env_value(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub fn create_oss_provider_with_base_url(base_url: &str, wire_api: WireApi) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "gpt-oss".into(),
        base_url: Some(base_url.into()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api,
        compat: None,
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

#[cfg(test)]
#[path = "model_provider_info_tests.rs"]
mod tests;

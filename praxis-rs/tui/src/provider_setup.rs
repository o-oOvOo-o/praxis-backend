use praxis_core::ModelProviderCompatInfo;
use praxis_core::ModelProviderInfo;
use praxis_core::ModelProviderThinkingFormat;
use praxis_core::WireApi;
use praxis_protocol::openai_models::ReasoningEffort;
use std::env;

pub(crate) const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
pub(crate) const COMMON_PROVIDER_ID: &str = "common";
pub(crate) const DEFAULT_DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub(crate) const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-v4-pro";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderSetupKind {
    DeepSeek,
    Common,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderSetupSelection {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) model: String,
    pub(crate) effort: Option<ReasoningEffort>,
}

#[derive(Default)]
struct ParsedProviderInput {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
}

impl ProviderSetupKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DeepSeek => "DeepSeek",
            Self::Common => "Common OpenAI Compatible",
        }
    }

    pub(crate) fn provider_id(self) -> &'static str {
        match self {
            Self::DeepSeek => DEEPSEEK_PROVIDER_ID,
            Self::Common => COMMON_PROVIDER_ID,
        }
    }

    pub(crate) fn default_base_url(self) -> &'static str {
        match self {
            Self::DeepSeek => DEFAULT_DEEPSEEK_BASE_URL,
            Self::Common => "",
        }
    }

    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Self::DeepSeek => DEFAULT_DEEPSEEK_MODEL,
            Self::Common => "",
        }
    }

    pub(crate) fn env_key(self) -> Option<&'static str> {
        match self {
            Self::DeepSeek => Some("DEEPSEEK_API_KEY"),
            Self::Common => None,
        }
    }

    pub(crate) fn default_effort(self) -> Option<ReasoningEffort> {
        match self {
            Self::DeepSeek => Some(ReasoningEffort::Medium),
            Self::Common => None,
        }
    }

    pub(crate) fn prefilled_api_key(self) -> Option<String> {
        self.env_key().and_then(|key| {
            env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
    }

    pub(crate) fn input_title(self) -> String {
        format!("Configure {}", self.label())
    }

    pub(crate) fn input_placeholder(self) -> String {
        match self {
            Self::DeepSeek => {
                "Paste API key. Optional lines: base_url=https://api.deepseek.com, model=deepseek-v4-pro"
                    .to_string()
            }
            Self::Common => {
                "api_key=...\nbase_url=https://provider.example/v1\nmodel=model-name"
                    .to_string()
            }
        }
    }

    pub(crate) fn input_context_label(self) -> Option<String> {
        match self {
            Self::DeepSeek => Some(format!(
                "Default: {} with {}",
                DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEK_MODEL
            )),
            Self::Common => Some("Required: api_key, base_url, model".to_string()),
        }
    }

    pub(crate) fn build_provider(self, api_key: String, base_url: String) -> ModelProviderInfo {
        let compat = match self {
            Self::DeepSeek => Some(ModelProviderCompatInfo {
                supports_developer_role: Some(false),
                supports_reasoning_effort: Some(true),
                thinking_format: Some(ModelProviderThinkingFormat::Deepseek),
                ..Default::default()
            }),
            Self::Common => None,
        };

        ModelProviderInfo {
            name: self.label().to_string(),
            base_url: Some(base_url),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: Some(api_key),
            auth: None,
            wire_api: WireApi::OpenAiCompat,
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

    pub(crate) fn parse_selection(self, raw: &str) -> Result<ProviderSetupSelection, String> {
        let mut parsed = parse_provider_input(raw)?;
        let api_key = take_required(&mut parsed.api_key, "API key")?;
        let base_url = parsed
            .base_url
            .take()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.default_base_url().to_string());
        let model = parsed
            .model
            .take()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.default_model().to_string());
        if base_url.trim().is_empty() {
            return Err(format!("Base URL is required for {}.", self.label()));
        }
        if model.trim().is_empty() {
            return Err(format!("Model is required for {}.", self.label()));
        }
        let base_url = normalize_base_url(&base_url)?;
        let model = model.trim().to_string();
        let provider = self.build_provider(api_key, base_url);
        Ok(ProviderSetupSelection {
            provider_id: self.provider_id().to_string(),
            provider,
            model,
            effort: self.default_effort(),
        })
    }
}

pub(crate) fn normalize_base_url(raw: &str) -> Result<String, String> {
    let base_url = raw.trim().trim_end_matches('/').to_string();
    if base_url.is_empty() {
        return Err("Base URL cannot be empty.".to_string());
    }
    if !base_url.starts_with("https://") && !base_url.starts_with("http://") {
        return Err("Base URL must start with http:// or https://.".to_string());
    }
    Ok(base_url)
}

fn parse_provider_input(raw: &str) -> Result<ParsedProviderInput, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Provider configuration cannot be empty.".to_string());
    }

    let mut parsed = ParsedProviderInput::default();
    let mut positionals = Vec::new();
    if raw.contains('=') {
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let whitespace_fields: Vec<&str> = line
                .split_whitespace()
                .filter(|field| field.contains('='))
                .collect();
            let fields: Vec<&str> = if whitespace_fields.len() > 1 {
                whitespace_fields
            } else {
                vec![line]
            };
            for field in fields {
                if let Some((key, value)) = field.split_once('=') {
                    assign_key_value(&mut parsed, key, value)?;
                } else {
                    positionals.push(field.to_string());
                }
            }
        }
    } else if raw.contains('|') {
        positionals.extend(
            raw.split('|')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        );
    } else {
        positionals.extend(
            raw.lines()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        );
        if positionals.len() == 1 {
            let whitespace_parts: Vec<String> = positionals[0]
                .split_whitespace()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            if whitespace_parts.len() >= 2 {
                positionals = whitespace_parts;
            }
        }
    }

    apply_positionals(&mut parsed, positionals);
    Ok(parsed)
}

fn assign_key_value(
    parsed: &mut ParsedProviderInput,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let key = key.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    let value = value.trim().to_string();
    if value.is_empty() {
        return Ok(());
    }
    match key.as_str() {
        "api_key" | "apikey" | "key" | "token" | "bearer_token" => {
            parsed.api_key = Some(value);
        }
        "base_url" | "url" | "endpoint" | "address" => {
            parsed.base_url = Some(value);
        }
        "model" | "model_name" => {
            parsed.model = Some(value);
        }
        other => {
            return Err(format!("Unknown provider config field `{other}`."));
        }
    }
    Ok(())
}

fn apply_positionals(parsed: &mut ParsedProviderInput, positionals: Vec<String>) {
    let mut parts = positionals.into_iter();
    if parsed.api_key.is_none() {
        parsed.api_key = parts.next();
    }
    if parsed.base_url.is_none() {
        parsed.base_url = parts.next();
    }
    if parsed.model.is_none() {
        parsed.model = parts.next();
    }
}

fn take_required(value: &mut Option<String>, label: &str) -> Result<String, String> {
    value
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} cannot be empty."))
}

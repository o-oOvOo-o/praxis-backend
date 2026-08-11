use praxis_core::ANTHROPIC_API_BASE_URL;
use praxis_core::ANTHROPIC_API_KEY_ENV_VAR;
use praxis_core::ANTHROPIC_PROVIDER_ID;
use praxis_core::KIMI_API_BASE_URL;
use praxis_core::KIMI_API_KEY_ENV_VAR;
use praxis_core::KIMI_PROVIDER_ID;
use praxis_core::ModelProviderCompatInfo;
use praxis_core::ModelProviderInfo;
use praxis_core::ModelProviderThinkingFormat;
use praxis_core::OPENAI_PROVIDER_ID;
use praxis_core::WireApi;
use praxis_login::ProviderApiKey;
use praxis_protocol::openai_models::ReasoningEffort;
use std::env;
use zeroize::Zeroize;
use zeroize::Zeroizing;

pub(crate) const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
pub(crate) const COMMON_PROVIDER_ID: &str = "common";
pub(crate) const RESPONSES_API_PROVIDER_ID: &str = "responses-api";
pub(crate) const CLAUDE_API_PROVIDER_ID: &str = "claude-api";
pub(crate) const COMMON_API_KEY_CREDENTIAL_ID: &str = "PRAXIS_COMMON_API_KEY";
pub(crate) const RESPONSES_API_KEY_ENV_VAR: &str = "PRAXIS_RESPONSES_API_KEY";
pub(crate) const CLAUDE_API_KEY_ENV_VAR: &str = "PRAXIS_CLAUDE_API_KEY";
pub(crate) const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
pub(crate) const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";
pub(crate) const DEFAULT_DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub(crate) const DEFAULT_OPENAI_MODEL: &str = "gpt-5.3-codex";
pub(crate) const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-v4-pro";
pub(crate) const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-5";
pub(crate) const DEFAULT_KIMI_MODEL: &str = "k3[1m]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderSetupKind {
    OpenAi,
    Anthropic,
    ResponsesApi,
    ClaudeApi,
    DeepSeek,
    Kimi,
    Common,
}

#[derive(Debug)]
pub(crate) struct ProviderSetupSelection {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) model: String,
    pub(crate) effort: Option<ReasoningEffort>,
    pub(crate) api_key: ProviderApiKey,
}

#[derive(Default)]
struct ParsedProviderInput {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
}

impl Drop for ParsedProviderInput {
    fn drop(&mut self) {
        if let Some(api_key) = self.api_key.as_mut() {
            api_key.zeroize();
        }
    }
}

impl ProviderSetupKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OpenAi => "Codex / OpenAI API key",
            Self::Anthropic => "Anthropic API key",
            Self::ResponsesApi => "Responses API",
            Self::ClaudeApi => "Claude API",
            Self::DeepSeek => "DeepSeek",
            Self::Kimi => "Kimi Code",
            Self::Common => "Common OpenAI Compatible",
        }
    }

    pub(crate) fn provider_id(self) -> &'static str {
        match self {
            Self::OpenAi => OPENAI_PROVIDER_ID,
            Self::Anthropic => ANTHROPIC_PROVIDER_ID,
            Self::ResponsesApi => RESPONSES_API_PROVIDER_ID,
            Self::ClaudeApi => CLAUDE_API_PROVIDER_ID,
            Self::DeepSeek => DEEPSEEK_PROVIDER_ID,
            Self::Kimi => KIMI_PROVIDER_ID,
            Self::Common => COMMON_PROVIDER_ID,
        }
    }

    pub(crate) fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAi | Self::ResponsesApi => OPENAI_API_BASE_URL,
            Self::Anthropic => ANTHROPIC_API_BASE_URL,
            Self::ClaudeApi => ANTHROPIC_API_BASE_URL,
            Self::DeepSeek => DEFAULT_DEEPSEEK_BASE_URL,
            Self::Kimi => KIMI_API_BASE_URL,
            Self::Common => "",
        }
    }

    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => DEFAULT_OPENAI_MODEL,
            Self::Anthropic => DEFAULT_ANTHROPIC_MODEL,
            Self::ResponsesApi | Self::ClaudeApi => "",
            Self::DeepSeek => DEFAULT_DEEPSEEK_MODEL,
            Self::Kimi => DEFAULT_KIMI_MODEL,
            Self::Common => "",
        }
    }

    pub(crate) fn env_key(self) -> &'static str {
        match self {
            Self::OpenAi => OPENAI_API_KEY_ENV_VAR,
            Self::Anthropic => ANTHROPIC_API_KEY_ENV_VAR,
            Self::ResponsesApi => RESPONSES_API_KEY_ENV_VAR,
            Self::ClaudeApi => CLAUDE_API_KEY_ENV_VAR,
            Self::DeepSeek => "DEEPSEEK_API_KEY",
            Self::Kimi => KIMI_API_KEY_ENV_VAR,
            Self::Common => COMMON_API_KEY_CREDENTIAL_ID,
        }
    }

    pub(crate) fn is_builtin(self) -> bool {
        matches!(self, Self::Kimi)
    }

    pub(crate) fn normalize_base_url(self, raw: &str) -> Result<String, String> {
        let base_url = normalize_base_url(raw)?;
        if matches!(self, Self::Kimi) && base_url != KIMI_API_BASE_URL {
            return Err(format!(
                "Kimi Code login uses the official endpoint {KIMI_API_BASE_URL}. Configure a separate custom provider for proxies."
            ));
        }
        Ok(base_url)
    }

    pub(crate) fn default_effort(self) -> Option<ReasoningEffort> {
        match self {
            Self::OpenAi => Some(ReasoningEffort::Medium),
            Self::Anthropic => Some(ReasoningEffort::High),
            Self::ResponsesApi | Self::ClaudeApi => None,
            Self::DeepSeek => Some(ReasoningEffort::Medium),
            Self::Kimi => Some(ReasoningEffort::Max),
            Self::Common => None,
        }
    }

    pub(crate) fn prefilled_api_key(self) -> Option<Zeroizing<String>> {
        let value = Zeroizing::new(env::var(self.env_key()).ok()?);
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| Zeroizing::new(trimmed.to_string()))
    }

    pub(crate) fn input_title(self) -> String {
        format!("Configure {}", self.label())
    }

    pub(crate) fn input_placeholder(self) -> String {
        match self {
            Self::OpenAi => format!(
                "api_key=...\nbase_url={OPENAI_API_BASE_URL}\nmodel={DEFAULT_OPENAI_MODEL}"
            ),
            Self::Anthropic => {
                format!(
                    "api_key=...\nbase_url={ANTHROPIC_API_BASE_URL}\nmodel={DEFAULT_ANTHROPIC_MODEL}"
                )
            }
            Self::ResponsesApi => format!(
                "api_key=...\nbase_url={OPENAI_API_BASE_URL}\nmodel=your-responses-model"
            ),
            Self::ClaudeApi => format!(
                "api_key=...\nbase_url={ANTHROPIC_API_BASE_URL}\nmodel=your-claude-model"
            ),
            Self::DeepSeek => {
                "Paste API key. Optional lines: base_url=https://api.deepseek.com, model=deepseek-v4-pro"
                    .to_string()
            }
            Self::Kimi => "Paste a Kimi Code API key. Optional: model=k3[1m]".to_string(),
            Self::Common => {
                "api_key=...\nbase_url=https://provider.example/v1\nmodel=model-name"
                    .to_string()
            }
        }
    }

    pub(crate) fn input_context_label(self) -> Option<String> {
        match self {
            Self::OpenAi => Some(
                "Uses the OpenAI Responses API and the same model catalog as Codex account login."
                    .to_string(),
            ),
            Self::Anthropic => Some(
                "Uses the Claude Messages API and the same model catalog as Claude account login."
                    .to_string(),
            ),
            Self::ResponsesApi => {
                Some("Configure a Responses endpoint and enter its model name.".to_string())
            }
            Self::ClaudeApi => {
                Some("Configure a Claude Messages endpoint and enter its model name.".to_string())
            }
            Self::DeepSeek => Some(format!(
                "Default: {} with {}",
                DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEK_MODEL
            )),
            Self::Kimi => Some(
                "Uses Kimi's Claude-compatible Coding endpoint. K3 1M requires Allegretto or higher."
                    .to_string(),
            ),
            Self::Common => Some("Required: api_key, base_url, model".to_string()),
        }
    }

    pub(crate) fn build_provider(self, base_url: String) -> ModelProviderInfo {
        if matches!(self, Self::OpenAi | Self::ResponsesApi) {
            let mut provider = ModelProviderInfo::create_openai_provider(Some(base_url));
            provider.name = self.label().to_string();
            provider.env_key = Some(self.env_key().to_string());
            provider.env_key_instructions = Some(format!(
                "Run `/login {}` to store this key in the operating system credential store, or set {}.",
                self.provider_id(),
                self.env_key()
            ));
            provider.requires_openai_auth = false;
            if matches!(self, Self::ResponsesApi) {
                provider.supports_websockets = false;
            }
            return provider;
        }
        if matches!(self, Self::Anthropic) {
            let mut provider = ModelProviderInfo::create_anthropic_provider();
            provider.base_url = Some(base_url);
            return provider;
        }
        if matches!(self, Self::ClaudeApi) {
            let mut provider = ModelProviderInfo::create_anthropic_provider();
            provider.name = self.label().to_string();
            provider.base_url = Some(base_url);
            provider.env_key = Some(self.env_key().to_string());
            provider.env_key_instructions = Some(format!(
                "Run `/login {}` to store this key in the operating system credential store, or set {}.",
                self.provider_id(),
                self.env_key()
            ));
            return provider;
        }
        if matches!(self, Self::Kimi) {
            return ModelProviderInfo::create_kimi_provider();
        }
        let compat = match self {
            Self::OpenAi | Self::ResponsesApi | Self::ClaudeApi => {
                unreachable!("specialized provider returned above")
            }
            Self::Anthropic => unreachable!("Anthropic provider returned above"),
            Self::Kimi => unreachable!("Kimi provider returned above"),
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
            env_key: Some(self.env_key().to_string()),
            env_key_instructions: Some(format!(
                "Run `/login {}` to store this key in the operating system credential store, or set {}.",
                self.provider_id(),
                self.env_key()
            )),
            experimental_bearer_token: None,
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
        let api_key = ProviderApiKey::new(api_key).map_err(|err| err.to_string())?;
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
        let base_url = self.normalize_base_url(&base_url)?;
        let model = model.trim().to_string();
        let provider = self.build_provider(base_url);
        Ok(ProviderSetupSelection {
            provider_id: self.provider_id().to_string(),
            provider,
            model,
            effort: self.default_effort(),
            api_key,
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
        if positionals.len() == 1
            && let Some(mut original) = positionals.pop()
        {
            let whitespace_parts: Vec<String> = original
                .split_whitespace()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            if whitespace_parts.len() >= 2 {
                original.zeroize();
                positionals = whitespace_parts;
            } else {
                positionals.push(original);
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
    let mut value = value.trim().to_string();
    if value.is_empty() {
        return Ok(());
    }
    match key.as_str() {
        "api_key" | "apikey" | "key" | "token" | "bearer_token" => {
            if let Some(previous) = parsed.api_key.as_mut() {
                previous.zeroize();
            }
            parsed.api_key = Some(value);
        }
        "base_url" | "url" | "endpoint" | "address" => {
            parsed.base_url = Some(value);
        }
        "model" | "model_name" => {
            parsed.model = Some(value);
        }
        other => {
            value.zeroize();
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
    for mut unused in parts {
        unused.zeroize();
    }
}

fn take_required(value: &mut Option<String>, label: &str) -> Result<String, String> {
    let Some(mut value) = value.take() else {
        return Err(format!("{label} cannot be empty."));
    };
    let trimmed = value.trim().to_string();
    value.zeroize();
    if trimmed.is_empty() {
        Err(format!("{label} cannot be empty."))
    } else {
        Ok(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_API_KEY: &str = "sk-ant-api03-test-only-never-use";

    #[test]
    fn anthropic_setup_keeps_api_key_out_of_provider_config_and_debug() {
        let selection = ProviderSetupKind::Anthropic
            .parse_selection(TEST_API_KEY)
            .expect("Anthropic provider setup");

        assert_eq!(selection.provider_id, ANTHROPIC_PROVIDER_ID);
        assert_eq!(selection.model, DEFAULT_ANTHROPIC_MODEL);
        assert_eq!(selection.provider.wire_api, WireApi::Claude);
        assert_eq!(
            selection.provider.env_key.as_deref(),
            Some(ANTHROPIC_API_KEY_ENV_VAR)
        );
        assert!(selection.provider.experimental_bearer_token.is_none());
        assert_eq!(selection.api_key.expose_secret(), TEST_API_KEY);
        assert_eq!(
            format!("{:?}", selection.api_key),
            "ProviderApiKey([REDACTED])"
        );

        let serialized = toml::to_string(&selection.provider).expect("serialize provider");
        assert!(!serialized.contains(TEST_API_KEY));
    }

    #[test]
    fn anthropic_setup_accepts_configurable_endpoint_under_shared_model_catalog() {
        let selection = ProviderSetupKind::Anthropic
            .parse_selection(&format!(
                "api_key={TEST_API_KEY} base_url=https://proxy.example.test"
            ))
            .expect("Anthropic API key endpoint");
        assert_eq!(selection.provider_id, ANTHROPIC_PROVIDER_ID);
        assert_eq!(
            selection.provider.base_url.as_deref(),
            Some("https://proxy.example.test")
        );
    }

    #[test]
    fn openai_api_key_reuses_official_provider_and_responses_wire() {
        let selection = ProviderSetupKind::OpenAi
            .parse_selection("openai-test-key")
            .expect("OpenAI provider setup");
        assert_eq!(selection.provider_id, OPENAI_PROVIDER_ID);
        assert_eq!(selection.model, DEFAULT_OPENAI_MODEL);
        assert_eq!(selection.provider.wire_api, WireApi::Responses);
        assert!(!selection.provider.requires_openai_auth);
        assert_eq!(
            selection.provider.env_key.as_deref(),
            Some(OPENAI_API_KEY_ENV_VAR)
        );
    }

    #[test]
    fn custom_wire_providers_require_model_and_keep_distinct_credentials() {
        let responses = ProviderSetupKind::ResponsesApi
            .parse_selection(
                "api_key=responses-key base_url=https://responses.example/v1 model=my-gpt",
            )
            .expect("Responses provider setup");
        let claude = ProviderSetupKind::ClaudeApi
            .parse_selection("api_key=claude-key base_url=https://claude.example model=my-claude")
            .expect("Claude provider setup");
        assert_eq!(responses.provider_id, RESPONSES_API_PROVIDER_ID);
        assert_eq!(responses.provider.wire_api, WireApi::Responses);
        assert_eq!(responses.model, "my-gpt");
        assert_eq!(claude.provider_id, CLAUDE_API_PROVIDER_ID);
        assert_eq!(claude.provider.wire_api, WireApi::Claude);
        assert_eq!(claude.model, "my-claude");
    }

    #[test]
    fn deepseek_setup_uses_keyring_credential_reference() {
        let selection = ProviderSetupKind::DeepSeek
            .parse_selection("deepseek-test-key")
            .expect("DeepSeek provider setup");
        assert_eq!(
            selection.provider.env_key.as_deref(),
            Some("DEEPSEEK_API_KEY")
        );
        assert!(selection.provider.experimental_bearer_token.is_none());
    }

    #[test]
    fn kimi_setup_uses_builtin_specialized_provider_and_keyring_reference() {
        let selection = ProviderSetupKind::Kimi
            .parse_selection("kimi-test-key")
            .expect("Kimi provider setup");

        assert_eq!(selection.provider_id, KIMI_PROVIDER_ID);
        assert_eq!(selection.model, DEFAULT_KIMI_MODEL);
        assert_eq!(selection.effort, Some(ReasoningEffort::Max));
        assert!(selection.provider.is_kimi());
        assert_eq!(selection.provider.wire_api, WireApi::Claude);
        assert_eq!(
            selection.provider.env_key.as_deref(),
            Some(KIMI_API_KEY_ENV_VAR)
        );
        assert!(selection.provider.experimental_bearer_token.is_none());
    }
}

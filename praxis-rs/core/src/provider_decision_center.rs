//! Request-time provider/runtime decision center.
//!
//! This is the single path that turns a static `ModelProviderInfo` plus the
//! current auth store into an executable request setup. Product surfaces such
//! as native app hosts should not decide API keys, auth realms, provider headers,
//! endpoint defaults, or telemetry snapshots on their own.

use std::sync::Arc;
use std::time::Duration;

use http::HeaderMap;
use http::header::HeaderName;
use http::header::HeaderValue;
use praxis_api::Provider as ApiProvider;
use praxis_api::provider::RetryConfig as ApiRetryConfig;
use praxis_login::AuthManager;
use praxis_login::AuthMode;
use praxis_login::CodexAuth;
use praxis_login::read_openai_api_key_from_env;

use crate::api_bridge::CoreAuthProvider;
use crate::auth_env_telemetry::AuthEnvTelemetry;
use crate::auth_env_telemetry::collect_auth_env_telemetry;
use crate::error::EnvVarError;
use crate::error::PraxisErr;
use crate::error::Result;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::WireApi;
use crate::provider_auth::auth_manager_for_provider;

const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";
const CHATGPT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthRequestPurpose {
    ModelTurn,
    ModelList,
    Realtime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderInterface {
    CodexResponsesLogin,
    ResponsesApiKey,
    OpenAiCompatible,
    ClaudeCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AuthDecisionSource {
    ProviderEnvKey(String),
    ProviderInlineBearer,
    ProviderCommand,
    ManagedOpenAi,
    OpenAiEnvRealtimeFallback,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthRealm {
    ProviderEnvironment,
    ProviderInlineConfig,
    ProviderCommand,
    ManagedOpenAi,
    OpenAiEnvironmentFallback,
    Anonymous,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProviderHeaderSource {
    Static { header: String },
    Environment { header: String, value_present: bool },
}

#[derive(Clone)]
pub(crate) struct AuthResolution {
    auth: Option<CodexAuth>,
    auth_mode: Option<AuthMode>,
    api_auth: CoreAuthProvider,
    source: AuthDecisionSource,
    realm: AuthRealm,
    interface: ProviderInterface,
}

pub(crate) struct ProviderRequestSetup {
    pub(crate) auth: Option<CodexAuth>,
    pub(crate) auth_mode: Option<AuthMode>,
    pub(crate) api_provider: ApiProvider,
    pub(crate) api_auth: CoreAuthProvider,
    pub(crate) source: AuthDecisionSource,
    pub(crate) realm: AuthRealm,
    pub(crate) interface: ProviderInterface,
    pub(crate) header_sources: Vec<ProviderHeaderSource>,
    pub(crate) auth_env_telemetry: AuthEnvTelemetry,
}

impl ProviderRequestSetup {
    pub(crate) fn header_source_labels(&self) -> Vec<String> {
        self.header_sources
            .iter()
            .map(ProviderHeaderSource::label)
            .collect()
    }
}

impl ProviderHeaderSource {
    fn label(&self) -> String {
        match self {
            ProviderHeaderSource::Static { header } => format!("{header}:static"),
            ProviderHeaderSource::Environment {
                header,
                value_present,
            } => {
                let status = if *value_present {
                    "env-present"
                } else {
                    "env-missing"
                };
                format!("{header}:{status}")
            }
        }
    }
}

struct ProviderHeaderResolution {
    headers: HeaderMap,
    sources: Vec<ProviderHeaderSource>,
}

struct ProviderEndpointResolution {
    api_provider: ApiProvider,
    header_sources: Vec<ProviderHeaderSource>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderDecisionCenter {
    auth_manager: Option<Arc<AuthManager>>,
}

impl ProviderDecisionCenter {
    pub(crate) fn new(auth_manager: Option<Arc<AuthManager>>) -> Self {
        Self { auth_manager }
    }

    pub(crate) fn provider_auth_manager(
        auth_manager: Option<Arc<AuthManager>>,
        provider: &ModelProviderInfo,
    ) -> Option<Arc<AuthManager>> {
        auth_manager_for_provider(auth_manager, provider)
    }

    pub(crate) fn auth_env_telemetry(&self, provider: &ModelProviderInfo) -> AuthEnvTelemetry {
        let praxis_api_key_env_enabled = self
            .auth_manager
            .as_ref()
            .is_some_and(|manager| manager.praxis_api_key_env_enabled());
        collect_auth_env_telemetry(provider, praxis_api_key_env_enabled)
    }

    pub(crate) async fn setup_provider(
        &self,
        provider: &ModelProviderInfo,
        purpose: AuthRequestPurpose,
    ) -> Result<ProviderRequestSetup> {
        let resolution = self.resolve(provider, purpose).await?;
        let endpoint = resolve_provider_endpoint(provider, resolution.auth_mode)?;
        Ok(ProviderRequestSetup {
            auth: resolution.auth,
            auth_mode: resolution.auth_mode,
            api_provider: endpoint.api_provider,
            api_auth: resolution.api_auth,
            source: resolution.source,
            realm: resolution.realm,
            interface: resolution.interface,
            header_sources: endpoint.header_sources,
            auth_env_telemetry: self.auth_env_telemetry(provider),
        })
    }

    pub(crate) async fn resolve(
        &self,
        provider: &ModelProviderInfo,
        purpose: AuthRequestPurpose,
    ) -> Result<AuthResolution> {
        let interface = ProviderInterface::from_provider(provider, None);

        if let Some((env_key, api_key)) = provider_env_api_key(provider)? {
            return Ok(AuthResolution {
                auth: None,
                auth_mode: Some(AuthMode::ApiKey),
                api_auth: CoreAuthProvider::new(Some(api_key), None),
                source: AuthDecisionSource::ProviderEnvKey(env_key),
                realm: AuthRealm::ProviderEnvironment,
                interface: ProviderInterface::from_provider(provider, Some(AuthMode::ApiKey)),
            });
        }

        if let Some(token) = provider.experimental_bearer_token.clone() {
            return Ok(AuthResolution {
                auth: None,
                auth_mode: Some(AuthMode::ApiKey),
                api_auth: CoreAuthProvider::new(Some(token), None),
                source: AuthDecisionSource::ProviderInlineBearer,
                realm: AuthRealm::ProviderInlineConfig,
                interface: ProviderInterface::from_provider(provider, Some(AuthMode::ApiKey)),
            });
        }

        if self.may_use_managed_auth(provider)
            && let Some(auth_manager) = self.auth_manager.as_ref()
            && let Some(auth) = auth_manager.auth().await
        {
            return self.resolve_from_managed_auth(provider, purpose, auth);
        }

        if matches!(purpose, AuthRequestPurpose::Realtime)
            && provider.is_openai()
            && let Some(api_key) = read_openai_api_key_from_env()
        {
            return Ok(AuthResolution {
                auth: None,
                auth_mode: Some(AuthMode::ApiKey),
                api_auth: CoreAuthProvider::new(Some(api_key), None),
                source: AuthDecisionSource::OpenAiEnvRealtimeFallback,
                realm: AuthRealm::OpenAiEnvironmentFallback,
                interface: ProviderInterface::ResponsesApiKey,
            });
        }

        Ok(AuthResolution {
            auth: None,
            auth_mode: None,
            api_auth: CoreAuthProvider::new(None, None),
            source: AuthDecisionSource::None,
            realm: AuthRealm::Anonymous,
            interface,
        })
    }

    fn may_use_managed_auth(&self, provider: &ModelProviderInfo) -> bool {
        provider.has_command_auth() || provider.requires_openai_auth || provider.is_openai()
    }

    fn resolve_from_managed_auth(
        &self,
        provider: &ModelProviderInfo,
        purpose: AuthRequestPurpose,
        auth: CodexAuth,
    ) -> Result<AuthResolution> {
        let auth_mode = auth.auth_mode();
        let source = if provider.has_command_auth() {
            AuthDecisionSource::ProviderCommand
        } else {
            AuthDecisionSource::ManagedOpenAi
        };
        let realm = if provider.has_command_auth() {
            AuthRealm::ProviderCommand
        } else {
            AuthRealm::ManagedOpenAi
        };
        let interface = ProviderInterface::from_provider(provider, Some(auth_mode));

        if matches!(purpose, AuthRequestPurpose::Realtime) {
            let Some(api_key) = auth.api_key().map(str::to_string) else {
                return Err(PraxisErr::InvalidRequest(
                    "realtime conversation requires API key auth".to_string(),
                ));
            };
            return Ok(AuthResolution {
                auth: Some(auth),
                auth_mode: Some(AuthMode::ApiKey),
                api_auth: CoreAuthProvider::new(Some(api_key), None),
                source,
                realm,
                interface: ProviderInterface::ResponsesApiKey,
            });
        }

        let token = auth.get_token()?;
        let account_id = auth.get_account_id();
        Ok(AuthResolution {
            auth: Some(auth),
            auth_mode: Some(auth_mode),
            api_auth: CoreAuthProvider::new(Some(token), account_id),
            source,
            realm,
            interface,
        })
    }
}

impl ProviderInterface {
    fn from_provider(provider: &ModelProviderInfo, auth_mode: Option<AuthMode>) -> Self {
        match provider.wire_api {
            WireApi::Responses => {
                if matches!(
                    auth_mode,
                    Some(AuthMode::Chatgpt | AuthMode::ChatgptAuthTokens)
                ) || provider.requires_openai_auth
                {
                    Self::CodexResponsesLogin
                } else {
                    Self::ResponsesApiKey
                }
            }
            WireApi::Claude => Self::ClaudeCode,
            WireApi::Common => Self::OpenAiCompatible,
        }
    }
}

fn resolve_provider_endpoint(
    provider: &ModelProviderInfo,
    auth_mode: Option<AuthMode>,
) -> Result<ProviderEndpointResolution> {
    let headers = resolve_provider_headers(provider)?;
    let default_base_url = if matches!(
        auth_mode,
        Some(AuthMode::Chatgpt | AuthMode::ChatgptAuthTokens)
    ) {
        CHATGPT_CODEX_BASE_URL
    } else {
        OPENAI_API_BASE_URL
    };
    let base_url = provider
        .base_url
        .clone()
        .unwrap_or_else(|| default_base_url.to_string());
    let api_provider = ApiProvider {
        name: provider.name.clone(),
        base_url,
        query_params: provider.query_params.clone(),
        headers: headers.headers,
        retry: ApiRetryConfig {
            max_attempts: provider.request_max_retries(),
            base_delay: Duration::from_millis(200),
            retry_429: false,
            retry_5xx: true,
            retry_transport: true,
        },
        stream_idle_timeout: provider.stream_idle_timeout(),
    };

    Ok(ProviderEndpointResolution {
        api_provider,
        header_sources: headers.sources,
    })
}

fn resolve_provider_headers(provider: &ModelProviderInfo) -> Result<ProviderHeaderResolution> {
    let capacity = provider
        .http_headers
        .as_ref()
        .map_or(0, |headers| headers.len())
        + provider
            .env_http_headers
            .as_ref()
            .map_or(0, |headers| headers.len());
    let mut headers = HeaderMap::with_capacity(capacity);
    let mut sources = Vec::with_capacity(capacity);

    if let Some(static_headers) = provider.http_headers.as_ref() {
        for (header, value) in static_headers {
            let name = parse_provider_header_name(provider, header)?;
            let value = parse_provider_header_value(provider, header, value)?;
            headers.insert(name, value);
            sources.push(ProviderHeaderSource::Static {
                header: header.clone(),
            });
        }
    }

    if let Some(env_headers) = provider.env_http_headers.as_ref() {
        for (header, env_var) in env_headers {
            let name = parse_provider_header_name(provider, header)?;
            let env_value = std::env::var(env_var)
                .ok()
                .filter(|value| !value.trim().is_empty());
            let value_present = env_value.is_some();
            sources.push(ProviderHeaderSource::Environment {
                header: header.clone(),
                value_present,
            });

            let Some(value) = env_value else {
                continue;
            };
            let value = parse_provider_header_value(provider, header, &value)?;
            headers.insert(name, value);
        }
    }

    Ok(ProviderHeaderResolution { headers, sources })
}

fn parse_provider_header_name(provider: &ModelProviderInfo, header: &str) -> Result<HeaderName> {
    HeaderName::try_from(header).map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "invalid http header name `{header}` in provider `{}`: {err}",
            provider.name
        ))
    })
}

fn parse_provider_header_value(
    provider: &ModelProviderInfo,
    header: &str,
    value: &str,
) -> Result<HeaderValue> {
    HeaderValue::try_from(value).map_err(|err| {
        PraxisErr::InvalidRequest(format!(
            "invalid http header value for `{header}` in provider `{}`: {err}",
            provider.name
        ))
    })
}

fn provider_env_api_key(provider: &ModelProviderInfo) -> Result<Option<(String, String)>> {
    let Some(env_key) = provider.env_key.as_ref() else {
        return Ok(None);
    };
    let api_key = std::env::var(env_key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            PraxisErr::EnvVar(EnvVarError {
                var: env_key.clone(),
                instructions: provider.env_key_instructions.clone(),
            })
        })?;
    Ok(Some((env_key.clone(), api_key)))
}

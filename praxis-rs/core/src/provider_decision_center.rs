//! Request-time provider/runtime decision center.
//!
//! This is the single path that turns a static `ModelProviderInfo` plus the
//! current auth store into an executable request setup. Product surfaces such
//! as native app hosts should not decide API keys, auth realms, provider headers,
//! endpoint defaults, or telemetry snapshots on their own.

mod auth_resolution;
mod endpoint;

use std::sync::Arc;

use praxis_api::Provider as ApiProvider;
use praxis_login::AuthManager;
use praxis_login::AuthMode;
use praxis_login::OpenAiAccountAuth;

use crate::api_bridge::CoreAuthProvider;
use crate::auth_env_telemetry::AuthEnvTelemetry;
use crate::auth_env_telemetry::collect_auth_env_telemetry;
use crate::error::Result;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::WireApi;
use crate::provider_auth::auth_manager_for_provider;

use endpoint::resolve_provider_endpoint;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthRequestPurpose {
    ModelTurn,
    ModelList,
    Realtime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderInterface {
    OpenAiResponsesLogin,
    ResponsesApiKey,
    OpenAiCompatible,
    ClaudeMessages,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AuthDecisionSource {
    AnthropicOauthCredentialStore,
    ProviderEnvKey(String),
    ProviderCredentialStore(String),
    ProviderInlineBearer,
    ProviderCommand,
    ManagedOpenAi,
    OpenAiEnvRealtimeFallback,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthRealm {
    AnthropicOauth,
    ProviderEnvironment,
    ProviderCredentialStore,
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
    auth: Option<OpenAiAccountAuth>,
    auth_mode: Option<AuthMode>,
    api_auth: CoreAuthProvider,
    source: AuthDecisionSource,
    realm: AuthRealm,
    interface: ProviderInterface,
}

pub(crate) struct ProviderRequestSetup {
    pub(crate) auth: Option<OpenAiAccountAuth>,
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
        provider_id: &str,
        provider: &ModelProviderInfo,
        purpose: AuthRequestPurpose,
    ) -> Result<ProviderRequestSetup> {
        let resolution = self.resolve(provider_id, provider, purpose).await?;
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
}

impl ProviderInterface {
    fn from_provider(provider: &ModelProviderInfo, auth_mode: Option<AuthMode>) -> Self {
        match provider.wire_api {
            WireApi::Responses => {
                if is_chatgpt_auth_mode(auth_mode) || provider.requires_openai_auth {
                    Self::OpenAiResponsesLogin
                } else {
                    Self::ResponsesApiKey
                }
            }
            WireApi::Claude => Self::ClaudeMessages,
            WireApi::OpenAiCompat => Self::OpenAiCompatible,
        }
    }
}

fn is_chatgpt_auth_mode(auth_mode: Option<AuthMode>) -> bool {
    matches!(
        auth_mode,
        Some(AuthMode::Chatgpt | AuthMode::ChatgptAuthTokens)
    )
}

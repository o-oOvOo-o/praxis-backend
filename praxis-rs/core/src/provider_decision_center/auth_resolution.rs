use praxis_login::AuthMode;
use praxis_login::OpenAiAccountAuth;
use praxis_login::read_openai_api_key_from_env;

use super::AuthDecisionSource;
use super::AuthRealm;
use super::AuthRequestPurpose;
use super::AuthResolution;
use super::ProviderDecisionCenter;
use super::ProviderInterface;
use crate::api_bridge::CoreAuthProvider;
use crate::error::EnvVarError;
use crate::error::PraxisErr;
use crate::error::Result;
use crate::model_provider_info::ModelProviderInfo;

impl ProviderDecisionCenter {
    pub(crate) async fn resolve(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
        purpose: AuthRequestPurpose,
    ) -> Result<AuthResolution> {
        let interface = ProviderInterface::from_provider(provider, None);

        if provider.is_anthropic()
            && let Some(auth_manager) = self.auth_manager.as_ref()
        {
            let oauth = auth_manager
                .anthropic_oauth_token()
                .await
                .map_err(|error| {
                    PraxisErr::Io(std::io::Error::other(format!(
                        "failed to resolve Anthropic OAuth credential: {error}"
                    )))
                })?;
            if let Some(token) = oauth {
                return Ok(AuthResolution {
                    auth: None,
                    auth_mode: None,
                    api_auth: CoreAuthProvider::from_anthropic_oauth(token),
                    source: AuthDecisionSource::AnthropicOauthCredentialStore,
                    realm: AuthRealm::AnthropicOauth,
                    interface: ProviderInterface::ClaudeMessages,
                });
            }
        }

        if let Some((source, realm, api_auth)) = self.provider_api_key(provider_id, provider)? {
            return Ok(AuthResolution {
                auth: None,
                auth_mode: Some(AuthMode::ApiKey),
                api_auth,
                source,
                realm,
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
        provider.can_use_managed_auth()
    }

    fn provider_api_key(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Result<Option<(AuthDecisionSource, AuthRealm, CoreAuthProvider)>> {
        let Some(env_key) = provider.env_key.as_ref() else {
            return Ok(None);
        };
        if let Some(api_key) = std::env::var(env_key)
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            let api_key = praxis_login::ProviderApiKey::new(api_key).map_err(|_| {
                PraxisErr::InvalidRequest(format!(
                    "environment variable `{env_key}` contains an invalid provider API key"
                ))
            })?;
            return Ok(Some((
                AuthDecisionSource::ProviderEnvKey(env_key.clone()),
                AuthRealm::ProviderEnvironment,
                CoreAuthProvider::from_provider_api_key(api_key, provider.wire_api),
            )));
        }

        if let Some(auth_manager) = self.auth_manager.as_ref() {
            let credential_id =
                praxis_login::provider_api_key_credential_id(provider_id).map_err(|err| {
                    PraxisErr::InvalidRequest(format!(
                        "invalid provider ID `{provider_id}` for credential storage: {err}"
                    ))
                })?;
            let stored = auth_manager.provider_api_key(&credential_id).map_err(|err| {
                PraxisErr::Io(std::io::Error::other(format!(
                    "failed to load provider credential `{credential_id}` from the operating system credential store: {err}"
                )))
            })?;
            if let Some(api_key) = stored {
                return Ok(Some((
                    AuthDecisionSource::ProviderCredentialStore(credential_id.clone()),
                    AuthRealm::ProviderCredentialStore,
                    CoreAuthProvider::from_provider_api_key(api_key, provider.wire_api),
                )));
            }
        }

        Err(PraxisErr::EnvVar(EnvVarError {
            var: env_key.clone(),
            instructions: provider.env_key_instructions.clone(),
        }))
    }

    fn resolve_from_managed_auth(
        &self,
        provider: &ModelProviderInfo,
        purpose: AuthRequestPurpose,
        auth: OpenAiAccountAuth,
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

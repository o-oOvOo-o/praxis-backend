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
        provider.can_use_managed_auth()
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

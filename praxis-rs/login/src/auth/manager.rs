use async_trait::async_trait;
use chrono::Utc;
#[cfg(test)]
use serial_test::serial;
use std::fmt::Debug;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use tokio::sync::Mutex as AsyncMutex;

use praxis_protocol::auth::AuthMode as ApiAuthMode;
use praxis_protocol::config_types::ModelProviderAuthInfo;

use super::external_bearer::BearerTokenRefresher;
use crate::auth::error::RefreshTokenFailedError;
use crate::auth::error::RefreshTokenFailedReason;
pub use crate::auth::storage::AuthCredentialsStoreMode;
pub use crate::auth::storage::AuthDotJson;
use crate::auth::storage::AuthStorageBackend;
use crate::auth::storage::create_auth_storage;
use crate::default_client::create_client;
use crate::token_data::KnownPlan as InternalKnownPlan;
use crate::token_data::PlanType as InternalPlanType;
use crate::token_data::TokenData;
use crate::token_data::parse_chatgpt_jwt_claims;
use crate::token_data::parse_jwt_expiration;
use praxis_client::PraxisHttpClient;
use praxis_protocol::account::PlanType as AccountPlanType;
use thiserror::Error;

mod recovery;
mod refresh;
mod source;

use recovery::ReloadOutcome;
pub use recovery::{UnauthorizedRecovery, UnauthorizedRecoveryStepResult};
#[cfg(test)]
use recovery::{UnauthorizedRecoveryMode, UnauthorizedRecoveryStep};
pub use refresh::CLIENT_ID;
use refresh::request_chatgpt_token_refresh;
pub use source::{
    AuthConfig, OPENAI_API_KEY_ENV_VAR, PRAXIS_API_KEY_ENV_VAR, enforce_login_restrictions,
    load_auth_dot_json, login_with_api_key, login_with_chatgpt_auth_tokens, logout,
    read_openai_api_key_from_env, read_praxis_api_key_from_env, save_auth,
};
use source::{load_auth, logout_all_stores, persist_tokens};

/// Authentication mechanism used by the current user.
#[derive(Debug, Clone)]
pub enum OpenAiAccountAuth {
    ApiKey(ApiKeyAuth),
    Chatgpt(ChatgptAuth),
    ChatgptAuthTokens(ChatgptAuthTokens),
}

#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    api_key: String,
}

#[derive(Debug, Clone)]
pub struct ChatgptAuth {
    state: ChatgptAuthState,
    storage: Arc<dyn AuthStorageBackend>,
}

#[derive(Debug, Clone)]
pub struct ChatgptAuthTokens {
    state: ChatgptAuthState,
}

#[derive(Debug, Clone)]
struct ChatgptAuthState {
    auth_dot_json: Arc<Mutex<Option<AuthDotJson>>>,
    client: PraxisHttpClient,
}

impl PartialEq for OpenAiAccountAuth {
    fn eq(&self, other: &Self) -> bool {
        self.api_auth_mode() == other.api_auth_mode()
    }
}

const TOKEN_REFRESH_INTERVAL: i64 = 8;

const REFRESH_TOKEN_EXPIRED_MESSAGE: &str = "Your access token could not be refreshed because your refresh token has expired. Please log out and sign in again.";
const REFRESH_TOKEN_REUSED_MESSAGE: &str = "Your access token could not be refreshed because your refresh token was already used. Please log out and sign in again.";
const REFRESH_TOKEN_INVALIDATED_MESSAGE: &str = "Your access token could not be refreshed because your refresh token was revoked. Please log out and sign in again.";
const REFRESH_TOKEN_UNKNOWN_MESSAGE: &str =
    "Your access token could not be refreshed. Please log out and sign in again.";
const REFRESH_TOKEN_ACCOUNT_MISMATCH_MESSAGE: &str = "Your access token could not be refreshed because you have since logged out or signed in to another account. Please sign in again.";
const REFRESH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR: &str = "PRAXIS_REFRESH_TOKEN_URL_OVERRIDE";

#[derive(Debug, Error)]
pub enum RefreshTokenError {
    #[error("{0}")]
    Permanent(#[from] RefreshTokenFailedError),
    #[error(transparent)]
    Transient(#[from] std::io::Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalAuthTokens {
    pub access_token: String,
    pub chatgpt_metadata: Option<ExternalAuthChatgptMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalAuthChatgptMetadata {
    pub account_id: String,
    pub plan_type: Option<String>,
}

impl ExternalAuthTokens {
    pub fn access_token_only(access_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            chatgpt_metadata: None,
        }
    }

    pub fn chatgpt(
        access_token: impl Into<String>,
        chatgpt_account_id: impl Into<String>,
        chatgpt_plan_type: Option<String>,
    ) -> Self {
        Self {
            access_token: access_token.into(),
            chatgpt_metadata: Some(ExternalAuthChatgptMetadata {
                account_id: chatgpt_account_id.into(),
                plan_type: chatgpt_plan_type,
            }),
        }
    }

    pub fn chatgpt_metadata(&self) -> Option<&ExternalAuthChatgptMetadata> {
        self.chatgpt_metadata.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalAuthRefreshReason {
    Unauthorized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalAuthRefreshContext {
    pub reason: ExternalAuthRefreshReason,
    pub previous_account_id: Option<String>,
}

#[async_trait]
/// Pluggable auth provider used by `AuthManager` for externally managed auth flows.
///
/// Implementations may either resolve auth eagerly via `resolve()` or provide refreshed
/// credentials on demand via `refresh()`.
pub trait ExternalAuth: Send + Sync {
    /// Indicates which top-level auth mode this external provider supplies.
    fn auth_mode(&self) -> crate::AuthMode;

    /// Returns cached or immediately available auth, if this provider can resolve it synchronously
    /// from the caller's perspective.
    async fn resolve(&self) -> std::io::Result<Option<ExternalAuthTokens>> {
        Ok(None)
    }

    /// Refreshes auth in response to a manager-driven refresh attempt.
    async fn refresh(
        &self,
        context: ExternalAuthRefreshContext,
    ) -> std::io::Result<ExternalAuthTokens>;
}

impl RefreshTokenError {
    pub fn failed_reason(&self) -> Option<RefreshTokenFailedReason> {
        match self {
            Self::Permanent(error) => Some(error.reason),
            Self::Transient(_) => None,
        }
    }
}

impl From<RefreshTokenError> for std::io::Error {
    fn from(err: RefreshTokenError) -> Self {
        match err {
            RefreshTokenError::Permanent(failed) => std::io::Error::other(failed),
            RefreshTokenError::Transient(inner) => inner,
        }
    }
}

impl OpenAiAccountAuth {
    fn from_auth_dot_json(
        praxis_home: &Path,
        auth_dot_json: AuthDotJson,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
    ) -> std::io::Result<Self> {
        let auth_mode = auth_dot_json.resolved_mode();
        let client = create_client();
        if auth_mode == ApiAuthMode::ApiKey {
            let Some(api_key) = auth_dot_json.openai_api_key.as_deref() else {
                return Err(std::io::Error::other("API key auth is missing a key."));
            };
            return Ok(Self::from_api_key(api_key));
        }

        let storage_mode = auth_dot_json.storage_mode(auth_credentials_store_mode);
        let state = ChatgptAuthState {
            auth_dot_json: Arc::new(Mutex::new(Some(auth_dot_json))),
            client,
        };

        match auth_mode {
            ApiAuthMode::Chatgpt => {
                let storage = create_auth_storage(praxis_home.to_path_buf(), storage_mode);
                Ok(Self::Chatgpt(ChatgptAuth { state, storage }))
            }
            ApiAuthMode::ChatgptAuthTokens => {
                Ok(Self::ChatgptAuthTokens(ChatgptAuthTokens { state }))
            }
            ApiAuthMode::ApiKey => unreachable!("api key mode is handled above"),
        }
    }

    pub fn from_auth_storage(
        praxis_home: &Path,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
    ) -> std::io::Result<Option<Self>> {
        load_auth(
            praxis_home,
            /*enable_praxis_api_key_env*/ false,
            auth_credentials_store_mode,
        )
    }

    pub fn auth_mode(&self) -> crate::AuthMode {
        match self {
            Self::ApiKey(_) => crate::AuthMode::ApiKey,
            Self::Chatgpt(_) | Self::ChatgptAuthTokens(_) => crate::AuthMode::Chatgpt,
        }
    }

    pub fn api_auth_mode(&self) -> ApiAuthMode {
        match self {
            Self::ApiKey(_) => ApiAuthMode::ApiKey,
            Self::Chatgpt(_) => ApiAuthMode::Chatgpt,
            Self::ChatgptAuthTokens(_) => ApiAuthMode::ChatgptAuthTokens,
        }
    }

    pub fn is_api_key_auth(&self) -> bool {
        self.auth_mode() == crate::AuthMode::ApiKey
    }

    pub fn is_chatgpt_auth(&self) -> bool {
        self.auth_mode() == crate::AuthMode::Chatgpt
    }

    pub fn is_external_chatgpt_tokens(&self) -> bool {
        matches!(self, Self::ChatgptAuthTokens(_))
    }

    /// Returns `None` if `auth_mode() != AuthMode::ApiKey`.
    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::ApiKey(auth) => Some(auth.api_key.as_str()),
            Self::Chatgpt(_) | Self::ChatgptAuthTokens(_) => None,
        }
    }

    /// Returns `Err` if `is_chatgpt_auth()` is false.
    pub fn get_token_data(&self) -> Result<TokenData, std::io::Error> {
        let auth_dot_json: Option<AuthDotJson> = self.get_current_auth_json();
        match auth_dot_json {
            Some(AuthDotJson {
                tokens: Some(tokens),
                last_refresh: Some(_),
                ..
            }) => Ok(tokens),
            _ => Err(std::io::Error::other("Token data is not available.")),
        }
    }

    /// Returns the token string used for bearer authentication.
    pub fn get_token(&self) -> Result<String, std::io::Error> {
        match self {
            Self::ApiKey(auth) => Ok(auth.api_key.clone()),
            Self::Chatgpt(_) | Self::ChatgptAuthTokens(_) => {
                let access_token = self.get_token_data()?.access_token;
                Ok(access_token)
            }
        }
    }

    /// Returns `None` if `is_chatgpt_auth()` is false.
    pub fn get_account_id(&self) -> Option<String> {
        self.get_current_token_data().and_then(|t| t.account_id)
    }

    /// Returns `None` if `is_chatgpt_auth()` is false.
    pub fn get_account_email(&self) -> Option<String> {
        self.get_current_token_data().and_then(|t| t.id_token.email)
    }

    /// Returns `None` if `is_chatgpt_auth()` is false.
    pub fn get_chatgpt_user_id(&self) -> Option<String> {
        self.get_current_token_data()
            .and_then(|t| t.id_token.chatgpt_user_id)
    }

    /// Account-facing plan classification derived from the current token.
    /// Returns a high-level `AccountPlanType` (e.g., Free/Plus/Pro/Team/…)
    /// mapped from the ID token's internal plan value. Prefer this when you
    /// need to make UI or product decisions based on the user's subscription.
    /// When ChatGPT auth is active but the token omits the plan claim, report
    /// `Unknown` instead of treating the account as invalid.
    pub fn account_plan_type(&self) -> Option<AccountPlanType> {
        let map_known = |kp: &InternalKnownPlan| match kp {
            InternalKnownPlan::Free => AccountPlanType::Free,
            InternalKnownPlan::Go => AccountPlanType::Go,
            InternalKnownPlan::Plus => AccountPlanType::Plus,
            InternalKnownPlan::Pro => AccountPlanType::Pro,
            InternalKnownPlan::Team => AccountPlanType::Team,
            InternalKnownPlan::SelfServeBusinessUsageBased => {
                AccountPlanType::SelfServeBusinessUsageBased
            }
            InternalKnownPlan::Business => AccountPlanType::Business,
            InternalKnownPlan::EnterpriseCbpUsageBased => AccountPlanType::EnterpriseCbpUsageBased,
            InternalKnownPlan::Enterprise => AccountPlanType::Enterprise,
            InternalKnownPlan::Edu => AccountPlanType::Edu,
        };

        self.get_current_token_data().map(|t| {
            t.id_token
                .chatgpt_plan_type
                .map(|pt| match pt {
                    InternalPlanType::Known(k) => map_known(&k),
                    InternalPlanType::Unknown(_) => AccountPlanType::Unknown,
                })
                .unwrap_or(AccountPlanType::Unknown)
        })
    }

    /// Returns `None` if `is_chatgpt_auth()` is false.
    fn get_current_auth_json(&self) -> Option<AuthDotJson> {
        let state = match self {
            Self::Chatgpt(auth) => &auth.state,
            Self::ChatgptAuthTokens(auth) => &auth.state,
            Self::ApiKey(_) => return None,
        };
        #[expect(clippy::unwrap_used)]
        state.auth_dot_json.lock().unwrap().clone()
    }

    /// Returns `None` if `is_chatgpt_auth()` is false.
    fn get_current_token_data(&self) -> Option<TokenData> {
        self.get_current_auth_json().and_then(|t| t.tokens)
    }

    /// Consider this private to integration tests.
    pub fn create_dummy_chatgpt_auth_for_testing() -> Self {
        let auth_dot_json = AuthDotJson {
            auth_mode: Some(ApiAuthMode::Chatgpt),
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: Default::default(),
                access_token: "Access Token".to_string(),
                refresh_token: "test".to_string(),
                account_id: Some("account_id".to_string()),
            }),
            last_refresh: Some(Utc::now()),
        };

        let client = create_client();
        let state = ChatgptAuthState {
            auth_dot_json: Arc::new(Mutex::new(Some(auth_dot_json))),
            client,
        };
        let storage = create_auth_storage(PathBuf::new(), AuthCredentialsStoreMode::File);
        Self::Chatgpt(ChatgptAuth { state, storage })
    }

    pub fn from_api_key(api_key: &str) -> Self {
        Self::ApiKey(ApiKeyAuth {
            api_key: api_key.to_owned(),
        })
    }
}

impl ChatgptAuth {
    fn current_auth_json(&self) -> Option<AuthDotJson> {
        #[expect(clippy::unwrap_used)]
        self.state.auth_dot_json.lock().unwrap().clone()
    }

    fn current_token_data(&self) -> Option<TokenData> {
        self.current_auth_json().and_then(|auth| auth.tokens)
    }

    fn storage(&self) -> &Arc<dyn AuthStorageBackend> {
        &self.storage
    }

    fn client(&self) -> &PraxisHttpClient {
        &self.state.client
    }
}

impl AuthDotJson {
    fn from_external_tokens(external: &ExternalAuthTokens) -> std::io::Result<Self> {
        let Some(chatgpt_metadata) = external.chatgpt_metadata() else {
            return Err(std::io::Error::other(
                "external auth tokens are missing ChatGPT metadata",
            ));
        };
        let mut token_info =
            parse_chatgpt_jwt_claims(&external.access_token).map_err(std::io::Error::other)?;
        token_info.chatgpt_account_id = Some(chatgpt_metadata.account_id.clone());
        token_info.chatgpt_plan_type = chatgpt_metadata
            .plan_type
            .as_deref()
            .map(InternalPlanType::from_raw_value)
            .or(token_info.chatgpt_plan_type)
            .or(Some(InternalPlanType::Unknown("unknown".to_string())));
        let tokens = TokenData {
            id_token: token_info,
            access_token: external.access_token.clone(),
            refresh_token: String::new(),
            account_id: Some(chatgpt_metadata.account_id.clone()),
        };

        Ok(Self {
            auth_mode: Some(ApiAuthMode::ChatgptAuthTokens),
            openai_api_key: None,
            tokens: Some(tokens),
            last_refresh: Some(Utc::now()),
        })
    }

    fn from_external_access_token(
        access_token: &str,
        chatgpt_account_id: &str,
        chatgpt_plan_type: Option<&str>,
    ) -> std::io::Result<Self> {
        let external = ExternalAuthTokens::chatgpt(
            access_token,
            chatgpt_account_id,
            chatgpt_plan_type.map(str::to_string),
        );
        Self::from_external_tokens(&external)
    }

    fn resolved_mode(&self) -> ApiAuthMode {
        if let Some(mode) = self.auth_mode {
            return mode;
        }
        if self.openai_api_key.is_some() {
            return ApiAuthMode::ApiKey;
        }
        ApiAuthMode::Chatgpt
    }

    fn storage_mode(
        &self,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
    ) -> AuthCredentialsStoreMode {
        if self.resolved_mode() == ApiAuthMode::ChatgptAuthTokens {
            AuthCredentialsStoreMode::Ephemeral
        } else {
            auth_credentials_store_mode
        }
    }
}

/// Internal cached auth state.
#[derive(Clone)]
struct CachedAuth {
    auth: Option<OpenAiAccountAuth>,
    /// Permanent refresh failure cached for the current auth snapshot so
    /// later refresh attempts for the same credentials fail fast without network.
    permanent_refresh_failure: Option<AuthScopedRefreshFailure>,
}

#[derive(Clone)]
struct AuthScopedRefreshFailure {
    auth: OpenAiAccountAuth,
    error: RefreshTokenFailedError,
}

impl Debug for CachedAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedAuth")
            .field(
                "auth_mode",
                &self.auth.as_ref().map(OpenAiAccountAuth::api_auth_mode),
            )
            .field(
                "permanent_refresh_failure",
                &self
                    .permanent_refresh_failure
                    .as_ref()
                    .map(|failure| failure.error.reason),
            )
            .finish()
    }
}

/// Central manager providing a single source of truth for auth.json derived
/// authentication data. It loads once (or on preference change) and then
/// hands out cloned `OpenAiAccountAuth` values so the rest of the program has a
/// consistent snapshot.
///
/// External modifications to `auth.json` will NOT be observed until
/// `reload()` is called explicitly. This matches the design goal of avoiding
/// different parts of the program seeing inconsistent auth data mid‑run.
pub struct AuthManager {
    praxis_home: PathBuf,
    inner: RwLock<CachedAuth>,
    enable_praxis_api_key_env: bool,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
    forced_chatgpt_workspace_id: RwLock<Option<String>>,
    refresh_lock: AsyncMutex<()>,
    anthropic_oauth_lock: AsyncMutex<()>,
    external_auth: RwLock<Option<Arc<dyn ExternalAuth>>>,
}

impl Debug for AuthManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthManager")
            .field("praxis_home", &self.praxis_home)
            .field("inner", &self.inner)
            .field("enable_praxis_api_key_env", &self.enable_praxis_api_key_env)
            .field(
                "auth_credentials_store_mode",
                &self.auth_credentials_store_mode,
            )
            .field(
                "forced_chatgpt_workspace_id",
                &self.forced_chatgpt_workspace_id,
            )
            .field("has_external_auth", &self.has_external_auth())
            .finish_non_exhaustive()
    }
}

impl AuthManager {
    /// Create a new manager loading the initial auth using the provided
    /// preferred auth method. Errors loading auth are swallowed; `auth()` will
    /// simply return `None` in that case so callers can treat it as an
    /// unauthenticated state.
    pub fn new(
        praxis_home: PathBuf,
        enable_praxis_api_key_env: bool,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
    ) -> Self {
        let managed_auth = load_auth(
            &praxis_home,
            enable_praxis_api_key_env,
            auth_credentials_store_mode,
        )
        .ok()
        .flatten();
        Self {
            praxis_home,
            inner: RwLock::new(CachedAuth {
                auth: managed_auth,
                permanent_refresh_failure: None,
            }),
            enable_praxis_api_key_env,
            auth_credentials_store_mode,
            forced_chatgpt_workspace_id: RwLock::new(None),
            refresh_lock: AsyncMutex::new(()),
            anthropic_oauth_lock: AsyncMutex::new(()),
            external_auth: RwLock::new(None),
        }
    }

    /// Create an AuthManager with a specific OpenAiAccountAuth, for testing only.
    pub fn from_auth_for_testing(auth: OpenAiAccountAuth) -> Arc<Self> {
        let cached = CachedAuth {
            auth: Some(auth),
            permanent_refresh_failure: None,
        };

        Arc::new(Self {
            praxis_home: PathBuf::from("non-existent"),
            inner: RwLock::new(cached),
            enable_praxis_api_key_env: false,
            auth_credentials_store_mode: AuthCredentialsStoreMode::File,
            forced_chatgpt_workspace_id: RwLock::new(None),
            refresh_lock: AsyncMutex::new(()),
            anthropic_oauth_lock: AsyncMutex::new(()),
            external_auth: RwLock::new(None),
        })
    }

    /// Create an AuthManager with a specific OpenAiAccountAuth and Praxis home, for testing only.
    pub fn from_auth_for_testing_with_home(
        auth: OpenAiAccountAuth,
        praxis_home: PathBuf,
    ) -> Arc<Self> {
        let cached = CachedAuth {
            auth: Some(auth),
            permanent_refresh_failure: None,
        };
        Arc::new(Self {
            praxis_home,
            inner: RwLock::new(cached),
            enable_praxis_api_key_env: false,
            auth_credentials_store_mode: AuthCredentialsStoreMode::File,
            forced_chatgpt_workspace_id: RwLock::new(None),
            refresh_lock: AsyncMutex::new(()),
            anthropic_oauth_lock: AsyncMutex::new(()),
            external_auth: RwLock::new(None),
        })
    }

    pub fn external_bearer_only(config: ModelProviderAuthInfo) -> Arc<Self> {
        Arc::new(Self {
            praxis_home: PathBuf::from("non-existent"),
            inner: RwLock::new(CachedAuth {
                auth: None,
                permanent_refresh_failure: None,
            }),
            enable_praxis_api_key_env: false,
            auth_credentials_store_mode: AuthCredentialsStoreMode::File,
            forced_chatgpt_workspace_id: RwLock::new(None),
            refresh_lock: AsyncMutex::new(()),
            anthropic_oauth_lock: AsyncMutex::new(()),
            external_auth: RwLock::new(Some(
                Arc::new(BearerTokenRefresher::new(config)) as Arc<dyn ExternalAuth>
            )),
        })
    }

    /// Current cached auth (clone) without attempting a refresh.
    pub fn auth_cached(&self) -> Option<OpenAiAccountAuth> {
        self.inner.read().ok().and_then(|c| c.auth.clone())
    }

    pub fn refresh_failure_for_auth(
        &self,
        auth: &OpenAiAccountAuth,
    ) -> Option<RefreshTokenFailedError> {
        self.inner.read().ok().and_then(|cached| {
            cached
                .permanent_refresh_failure
                .as_ref()
                .filter(|failure| Self::auths_equal_for_refresh(Some(auth), Some(&failure.auth)))
                .map(|failure| failure.error.clone())
        })
    }

    /// Current cached auth (clone). May be `None` if not logged in or load failed.
    /// For stale managed ChatGPT auth, first performs a guarded reload and then
    /// refreshes only if the on-disk auth is unchanged.
    pub async fn auth(&self) -> Option<OpenAiAccountAuth> {
        if let Some(auth) = self.resolve_external_api_key_auth().await {
            return Some(auth);
        }

        let auth = self.auth_cached()?;
        if Self::is_stale_for_proactive_refresh(&auth)
            && let Err(err) = self.refresh_token().await
        {
            tracing::error!("Failed to refresh token: {}", err);
            return Some(auth);
        }
        self.auth_cached()
    }

    /// Force a reload of the auth information from auth.json. Returns
    /// whether the auth value changed.
    pub fn reload(&self) -> bool {
        tracing::info!("Reloading auth");
        let new_auth = self.load_auth_from_storage();
        self.set_cached_auth(new_auth)
    }

    fn reload_if_account_id_matches(&self, expected_account_id: Option<&str>) -> ReloadOutcome {
        let expected_account_id = match expected_account_id {
            Some(account_id) => account_id,
            None => {
                tracing::info!("Skipping auth reload because no account id is available.");
                return ReloadOutcome::Skipped;
            }
        };

        let new_auth = self.load_auth_from_storage();
        let new_account_id = new_auth
            .as_ref()
            .and_then(OpenAiAccountAuth::get_account_id);

        if new_account_id.as_deref() != Some(expected_account_id) {
            let found_account_id = new_account_id.as_deref().unwrap_or("unknown");
            tracing::info!(
                "Skipping auth reload due to account id mismatch (expected: {expected_account_id}, found: {found_account_id})"
            );
            return ReloadOutcome::Skipped;
        }

        tracing::info!("Reloading auth for account {expected_account_id}");
        let cached_before_reload = self.auth_cached();
        let auth_changed =
            !Self::auths_equal_for_refresh(cached_before_reload.as_ref(), new_auth.as_ref());
        self.set_cached_auth(new_auth);
        if auth_changed {
            ReloadOutcome::ReloadedChanged
        } else {
            ReloadOutcome::ReloadedNoChange
        }
    }

    fn auths_equal_for_refresh(
        a: Option<&OpenAiAccountAuth>,
        b: Option<&OpenAiAccountAuth>,
    ) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => match (a.api_auth_mode(), b.api_auth_mode()) {
                (ApiAuthMode::ApiKey, ApiAuthMode::ApiKey) => a.api_key() == b.api_key(),
                (ApiAuthMode::Chatgpt, ApiAuthMode::Chatgpt)
                | (ApiAuthMode::ChatgptAuthTokens, ApiAuthMode::ChatgptAuthTokens) => {
                    a.get_current_auth_json() == b.get_current_auth_json()
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn auths_equal(a: Option<&OpenAiAccountAuth>, b: Option<&OpenAiAccountAuth>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Records a permanent refresh failure only if the failed refresh was
    /// attempted against the auth snapshot that is still cached.
    fn record_permanent_refresh_failure_if_unchanged(
        &self,
        attempted_auth: &OpenAiAccountAuth,
        error: &RefreshTokenFailedError,
    ) {
        if let Ok(mut guard) = self.inner.write() {
            let current_auth_matches =
                Self::auths_equal_for_refresh(Some(attempted_auth), guard.auth.as_ref());
            if current_auth_matches {
                guard.permanent_refresh_failure = Some(AuthScopedRefreshFailure {
                    auth: attempted_auth.clone(),
                    error: error.clone(),
                });
            }
        }
    }

    fn load_auth_from_storage(&self) -> Option<OpenAiAccountAuth> {
        load_auth(
            &self.praxis_home,
            self.enable_praxis_api_key_env,
            self.auth_credentials_store_mode,
        )
        .ok()
        .flatten()
    }

    fn set_cached_auth(&self, new_auth: Option<OpenAiAccountAuth>) -> bool {
        if let Ok(mut guard) = self.inner.write() {
            let previous = guard.auth.as_ref();
            let changed = !AuthManager::auths_equal(previous, new_auth.as_ref());
            let auth_changed_for_refresh =
                !Self::auths_equal_for_refresh(previous, new_auth.as_ref());
            if auth_changed_for_refresh {
                guard.permanent_refresh_failure = None;
            }
            tracing::info!("Reloaded auth, changed: {changed}");
            guard.auth = new_auth;
            changed
        } else {
            false
        }
    }

    pub fn set_external_auth(&self, external_auth: Arc<dyn ExternalAuth>) {
        if let Ok(mut guard) = self.external_auth.write() {
            *guard = Some(external_auth);
        }
    }

    pub fn clear_external_auth(&self) {
        if let Ok(mut guard) = self.external_auth.write() {
            *guard = None;
        }
    }

    pub fn set_forced_chatgpt_workspace_id(&self, workspace_id: Option<String>) {
        if let Ok(mut guard) = self.forced_chatgpt_workspace_id.write() {
            *guard = workspace_id;
        }
    }

    pub fn forced_chatgpt_workspace_id(&self) -> Option<String> {
        self.forced_chatgpt_workspace_id
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn has_external_auth(&self) -> bool {
        self.external_auth().is_some()
    }

    pub fn is_external_chatgpt_auth_active(&self) -> bool {
        self.auth_cached()
            .as_ref()
            .is_some_and(OpenAiAccountAuth::is_external_chatgpt_tokens)
    }

    pub fn praxis_api_key_env_enabled(&self) -> bool {
        self.enable_praxis_api_key_env
    }

    /// Resolve a provider-scoped API key from the operating system credential store.
    pub fn provider_api_key(
        &self,
        credential_id: &str,
    ) -> Result<Option<crate::ProviderApiKey>, crate::ProviderApiKeyError> {
        crate::load_provider_api_key(&self.praxis_home, credential_id)
    }

    /// Resolve a Praxis-owned Anthropic OAuth token, importing the local Claude Code login once.
    pub async fn anthropic_oauth_token(
        &self,
    ) -> Result<Option<crate::AnthropicOauthAccessToken>, crate::AnthropicOauthError> {
        let _guard = self.anthropic_oauth_lock.lock().await;
        crate::load_import_and_refresh_anthropic_oauth(&self.praxis_home).await
    }

    /// Convenience constructor returning an `Arc` wrapper.
    pub fn shared(
        praxis_home: PathBuf,
        enable_praxis_api_key_env: bool,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
    ) -> Arc<Self> {
        Arc::new(Self::new(
            praxis_home,
            enable_praxis_api_key_env,
            auth_credentials_store_mode,
        ))
    }

    pub fn shared_with_external_auth(
        praxis_home: PathBuf,
        enable_praxis_api_key_env: bool,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
        external_auth: Arc<dyn ExternalAuth>,
    ) -> Arc<Self> {
        let manager = Self::shared(
            praxis_home,
            enable_praxis_api_key_env,
            auth_credentials_store_mode,
        );
        manager.set_external_auth(external_auth);
        manager
    }

    pub fn unauthorized_recovery(self: &Arc<Self>) -> UnauthorizedRecovery {
        UnauthorizedRecovery::new(Arc::clone(self))
    }

    fn external_auth(&self) -> Option<Arc<dyn ExternalAuth>> {
        self.external_auth
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().cloned())
    }

    fn external_auth_mode(&self) -> Option<crate::AuthMode> {
        self.external_auth()
            .as_ref()
            .map(|external_auth| external_auth.auth_mode())
    }

    fn has_external_api_key_auth(&self) -> bool {
        self.external_auth_mode() == Some(crate::AuthMode::ApiKey)
    }

    async fn resolve_external_api_key_auth(&self) -> Option<OpenAiAccountAuth> {
        if !self.has_external_api_key_auth() {
            return None;
        }

        let external_auth = self.external_auth()?;

        match external_auth.resolve().await {
            Ok(Some(tokens)) => Some(OpenAiAccountAuth::from_api_key(&tokens.access_token)),
            Ok(None) => None,
            Err(err) => {
                tracing::error!("Failed to resolve external API key auth: {err}");
                None
            }
        }
    }

    /// Attempt to refresh the token by first performing a guarded reload. Auth
    /// is reloaded from storage only when the account id matches the currently
    /// cached account id. If the persisted token differs from the cached token, we
    /// can assume that some other instance already refreshed it. If the persisted
    /// token is the same as the cached, then ask the token authority to refresh.
    pub async fn refresh_token(&self) -> Result<(), RefreshTokenError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        let auth_before_reload = self.auth_cached();
        if auth_before_reload
            .as_ref()
            .is_some_and(OpenAiAccountAuth::is_api_key_auth)
        {
            return Ok(());
        }
        let expected_account_id = auth_before_reload
            .as_ref()
            .and_then(OpenAiAccountAuth::get_account_id);

        match self.reload_if_account_id_matches(expected_account_id.as_deref()) {
            ReloadOutcome::ReloadedChanged => {
                tracing::info!("Skipping token refresh because auth changed after guarded reload.");
                Ok(())
            }
            ReloadOutcome::ReloadedNoChange => self.refresh_token_from_authority_impl().await,
            ReloadOutcome::Skipped => {
                Err(RefreshTokenError::Permanent(RefreshTokenFailedError::new(
                    RefreshTokenFailedReason::Other,
                    REFRESH_TOKEN_ACCOUNT_MISMATCH_MESSAGE.to_string(),
                )))
            }
        }
    }

    /// Attempt to refresh the current auth token from the authority that issued
    /// the token. On success, reloads the auth state from disk so other components
    /// observe refreshed token. If the token refresh fails, returns the error to
    /// the caller.
    pub async fn refresh_token_from_authority(&self) -> Result<(), RefreshTokenError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        self.refresh_token_from_authority_impl().await
    }

    async fn refresh_token_from_authority_impl(&self) -> Result<(), RefreshTokenError> {
        tracing::info!("Refreshing token");

        let auth = match self.auth_cached() {
            Some(auth) => auth,
            None => return Ok(()),
        };
        if let Some(error) = self.refresh_failure_for_auth(&auth) {
            return Err(RefreshTokenError::Permanent(error));
        }

        let attempted_auth = auth.clone();
        let result = match auth {
            OpenAiAccountAuth::ChatgptAuthTokens(_) if self.has_external_auth() => {
                self.refresh_external_auth(ExternalAuthRefreshReason::Unauthorized)
                    .await
            }
            OpenAiAccountAuth::ChatgptAuthTokens(_) => Ok(()),
            OpenAiAccountAuth::Chatgpt(chatgpt_auth) => {
                let token_data = chatgpt_auth.current_token_data().ok_or_else(|| {
                    RefreshTokenError::Transient(std::io::Error::other(
                        "Token data is not available.",
                    ))
                })?;
                self.refresh_and_persist_chatgpt_token(&chatgpt_auth, token_data.refresh_token)
                    .await
            }
            OpenAiAccountAuth::ApiKey(_) => Ok(()),
        };
        if let Err(RefreshTokenError::Permanent(error)) = &result {
            self.record_permanent_refresh_failure_if_unchanged(&attempted_auth, error);
        }
        result
    }

    /// Log out by deleting the on‑disk auth.json (if present). Returns Ok(true)
    /// if a file was removed, Ok(false) if no auth file existed. On success,
    /// reloads the in‑memory auth cache so callers immediately observe the
    /// unauthenticated state.
    pub fn logout(&self) -> std::io::Result<bool> {
        let removed = logout_all_stores(&self.praxis_home, self.auth_credentials_store_mode)?;
        // Always reload to clear any cached auth (even if file absent).
        self.reload();
        Ok(removed)
    }

    pub fn get_api_auth_mode(&self) -> Option<ApiAuthMode> {
        if self.has_external_api_key_auth() {
            return Some(ApiAuthMode::ApiKey);
        }
        self.auth_cached()
            .as_ref()
            .map(OpenAiAccountAuth::api_auth_mode)
    }

    pub fn auth_mode(&self) -> Option<crate::AuthMode> {
        if self.has_external_api_key_auth() {
            return Some(crate::AuthMode::ApiKey);
        }
        self.auth_cached()
            .as_ref()
            .map(OpenAiAccountAuth::auth_mode)
    }

    fn is_stale_for_proactive_refresh(auth: &OpenAiAccountAuth) -> bool {
        let chatgpt_auth = match auth {
            OpenAiAccountAuth::Chatgpt(chatgpt_auth) => chatgpt_auth,
            _ => return false,
        };

        let auth_dot_json = match chatgpt_auth.current_auth_json() {
            Some(auth_dot_json) => auth_dot_json,
            None => return false,
        };
        if let Some(tokens) = auth_dot_json.tokens.as_ref()
            && let Ok(Some(expires_at)) = parse_jwt_expiration(&tokens.access_token)
        {
            return expires_at <= Utc::now();
        }
        let last_refresh = match auth_dot_json.last_refresh {
            Some(last_refresh) => last_refresh,
            None => return false,
        };
        last_refresh < Utc::now() - chrono::Duration::days(TOKEN_REFRESH_INTERVAL)
    }

    async fn refresh_external_auth(
        &self,
        reason: ExternalAuthRefreshReason,
    ) -> Result<(), RefreshTokenError> {
        let Some(external_auth) = self.external_auth() else {
            return Err(RefreshTokenError::Transient(std::io::Error::other(
                "external auth is not configured",
            )));
        };
        let forced_chatgpt_workspace_id = self.forced_chatgpt_workspace_id();
        let previous_account_id = self
            .auth_cached()
            .as_ref()
            .and_then(OpenAiAccountAuth::get_account_id);
        let context = ExternalAuthRefreshContext {
            reason,
            previous_account_id,
        };

        let refreshed = external_auth
            .refresh(context)
            .await
            .map_err(RefreshTokenError::Transient)?;
        if external_auth.auth_mode() == crate::AuthMode::ApiKey {
            return Ok(());
        }
        let Some(chatgpt_metadata) = refreshed.chatgpt_metadata() else {
            return Err(RefreshTokenError::Transient(std::io::Error::other(
                "external auth refresh did not return ChatGPT metadata",
            )));
        };
        if let Some(expected_workspace_id) = forced_chatgpt_workspace_id.as_deref()
            && chatgpt_metadata.account_id != expected_workspace_id
        {
            return Err(RefreshTokenError::Transient(std::io::Error::other(
                format!(
                    "external auth refresh returned workspace {:?}, expected {expected_workspace_id:?}",
                    chatgpt_metadata.account_id,
                ),
            )));
        }
        let auth_dot_json =
            AuthDotJson::from_external_tokens(&refreshed).map_err(RefreshTokenError::Transient)?;
        save_auth(
            &self.praxis_home,
            &auth_dot_json,
            AuthCredentialsStoreMode::Ephemeral,
        )
        .map_err(RefreshTokenError::Transient)?;
        self.reload();
        Ok(())
    }

    // Refreshes ChatGPT OAuth tokens, persists the updated auth state, and
    // reloads the in-memory cache so callers immediately observe new tokens.
    async fn refresh_and_persist_chatgpt_token(
        &self,
        auth: &ChatgptAuth,
        refresh_token: String,
    ) -> Result<(), RefreshTokenError> {
        let refresh_response = request_chatgpt_token_refresh(refresh_token, auth.client()).await?;

        persist_tokens(
            auth.storage(),
            refresh_response.id_token,
            refresh_response.access_token,
            refresh_response.refresh_token,
        )
        .map_err(RefreshTokenError::from)?;
        self.reload();

        Ok(())
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;

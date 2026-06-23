use chrono::Utc;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::auth::AuthMode as ApiAuthMode;
use praxis_protocol::config_types::ForcedLoginMethod;

use super::OpenAiAccountAuth;
use crate::auth::storage::AuthCredentialsStoreMode;
use crate::auth::storage::AuthDotJson;
use crate::auth::storage::AuthStorageBackend;
use crate::auth::storage::LoadedAuthOrigin;
use crate::auth::storage::create_auth_storage;
use crate::auth::storage::load_persistent_auth_with_origin;
use crate::token_data::TokenData;
use crate::token_data::parse_chatgpt_jwt_claims;

pub const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
pub const PRAXIS_API_KEY_ENV_VAR: &str = "PRAXIS_API_KEY";
const LEGACY_CODEX_API_KEY_ENV_VAR: &str = "CODEX_API_KEY";

pub fn read_openai_api_key_from_env() -> Option<String> {
    env::var(OPENAI_API_KEY_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn read_praxis_api_key_from_env() -> Option<String> {
    [PRAXIS_API_KEY_ENV_VAR, LEGACY_CODEX_API_KEY_ENV_VAR]
        .into_iter()
        .find_map(|name| {
            env::var(name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

/// Delete the auth.json file inside `praxis_home` if it exists. Returns `Ok(true)`
/// if a file was removed, `Ok(false)` if no auth file was present.
pub fn logout(
    praxis_home: &Path,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<bool> {
    let storage = create_auth_storage(praxis_home.to_path_buf(), auth_credentials_store_mode);
    storage.delete()
}

/// Writes an `auth.json` that contains only the API key.
pub fn login_with_api_key(
    praxis_home: &Path,
    api_key: &str,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(ApiAuthMode::ApiKey),
        openai_api_key: Some(api_key.to_string()),
        tokens: None,
        last_refresh: None,
    };
    save_auth(praxis_home, &auth_dot_json, auth_credentials_store_mode)
}

/// Writes an in-memory auth payload for externally managed ChatGPT tokens.
pub fn login_with_chatgpt_auth_tokens(
    praxis_home: &Path,
    access_token: &str,
    chatgpt_account_id: &str,
    chatgpt_plan_type: Option<&str>,
) -> std::io::Result<()> {
    let auth_dot_json = AuthDotJson::from_external_access_token(
        access_token,
        chatgpt_account_id,
        chatgpt_plan_type,
    )?;
    save_auth(
        praxis_home,
        &auth_dot_json,
        AuthCredentialsStoreMode::Ephemeral,
    )
}

/// Persist the provided auth payload using the specified backend.
pub fn save_auth(
    praxis_home: &Path,
    auth: &AuthDotJson,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    let storage = create_auth_storage(praxis_home.to_path_buf(), auth_credentials_store_mode);
    storage.save(auth)
}

/// Load CLI auth data using the configured credential store backend.
/// Returns `None` when no credentials are stored. This function is
/// provided only for tests. Production code should not directly load
/// from the auth.json storage. It should use the AuthManager abstraction
/// instead.
pub fn load_auth_dot_json(
    praxis_home: &Path,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<Option<AuthDotJson>> {
    let storage = create_auth_storage(praxis_home.to_path_buf(), auth_credentials_store_mode);
    storage.load()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthConfig {
    pub praxis_home: PathBuf,
    pub auth_credentials_store_mode: AuthCredentialsStoreMode,
    pub forced_login_method: Option<ForcedLoginMethod>,
    pub forced_chatgpt_workspace_id: Option<String>,
}

pub fn enforce_login_restrictions(config: &AuthConfig) -> std::io::Result<()> {
    let Some(auth) = load_auth(
        &config.praxis_home,
        /*enable_praxis_api_key_env*/ true,
        config.auth_credentials_store_mode,
    )?
    else {
        return Ok(());
    };

    if let Some(required_method) = config.forced_login_method {
        let method_violation = match (required_method, auth.auth_mode()) {
            (ForcedLoginMethod::Api, crate::AuthMode::ApiKey) => None,
            (ForcedLoginMethod::Chatgpt, crate::AuthMode::Chatgpt)
            | (ForcedLoginMethod::Chatgpt, crate::AuthMode::ChatgptAuthTokens) => None,
            (ForcedLoginMethod::Api, crate::AuthMode::Chatgpt)
            | (ForcedLoginMethod::Api, crate::AuthMode::ChatgptAuthTokens) => Some(
                "API key login is required, but ChatGPT is currently being used. Logging out."
                    .to_string(),
            ),
            (ForcedLoginMethod::Chatgpt, crate::AuthMode::ApiKey) => Some(
                "ChatGPT login is required, but an API key is currently being used. Logging out."
                    .to_string(),
            ),
        };

        if let Some(message) = method_violation {
            return logout_with_message(
                &config.praxis_home,
                message,
                config.auth_credentials_store_mode,
            );
        }
    }

    if let Some(expected_account_id) = config.forced_chatgpt_workspace_id.as_deref() {
        if !auth.is_chatgpt_auth() {
            return Ok(());
        }

        let token_data = match auth.get_token_data() {
            Ok(data) => data,
            Err(err) => {
                return logout_with_message(
                    &config.praxis_home,
                    format!(
                        "Failed to load ChatGPT credentials while enforcing workspace restrictions: {err}. Logging out."
                    ),
                    config.auth_credentials_store_mode,
                );
            }
        };

        // workspace is the external identifier for account id.
        let chatgpt_account_id = token_data.id_token.chatgpt_account_id.as_deref();
        if chatgpt_account_id != Some(expected_account_id) {
            let message = match chatgpt_account_id {
                Some(actual) => format!(
                    "Login is restricted to workspace {expected_account_id}, but current credentials belong to {actual}. Logging out."
                ),
                None => format!(
                    "Login is restricted to workspace {expected_account_id}, but current credentials lack a workspace identifier. Logging out."
                ),
            };
            return logout_with_message(
                &config.praxis_home,
                message,
                config.auth_credentials_store_mode,
            );
        }
    }

    Ok(())
}

fn logout_with_message(
    praxis_home: &Path,
    message: String,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    // External auth tokens live in the ephemeral store, but persistent auth may still exist
    // from earlier logins. Clear both so a forced logout truly removes all active auth.
    let removal_result = logout_all_stores(praxis_home, auth_credentials_store_mode);
    let error_message = match removal_result {
        Ok(_) => message,
        Err(err) => format!("{message}. Failed to remove auth.json: {err}"),
    };
    Err(std::io::Error::other(error_message))
}

pub(super) fn logout_all_stores(
    praxis_home: &Path,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<bool> {
    if auth_credentials_store_mode == AuthCredentialsStoreMode::Ephemeral {
        return logout(praxis_home, AuthCredentialsStoreMode::Ephemeral);
    }
    let removed_ephemeral = logout(praxis_home, AuthCredentialsStoreMode::Ephemeral)?;
    let removed_managed = logout(praxis_home, auth_credentials_store_mode)?;
    Ok(removed_ephemeral || removed_managed)
}

pub(super) fn load_auth(
    praxis_home: &Path,
    enable_praxis_api_key_env: bool,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<Option<OpenAiAccountAuth>> {
    let build_auth = |auth_dot_json: AuthDotJson, storage_mode| {
        OpenAiAccountAuth::from_auth_dot_json(praxis_home, auth_dot_json, storage_mode)
    };

    // API key via env var takes precedence over any other auth method.
    if enable_praxis_api_key_env && let Some(api_key) = read_praxis_api_key_from_env() {
        return Ok(Some(OpenAiAccountAuth::from_api_key(api_key.as_str())));
    }

    // External ChatGPT auth tokens live in the in-memory (ephemeral) store. Always check this
    // first so external auth takes precedence over any persisted credentials.
    let ephemeral_storage = create_auth_storage(
        praxis_home.to_path_buf(),
        AuthCredentialsStoreMode::Ephemeral,
    );
    if let Some(auth_dot_json) = ephemeral_storage.load()? {
        let auth = build_auth(auth_dot_json, AuthCredentialsStoreMode::Ephemeral)?;
        return Ok(Some(auth));
    }

    // If the caller explicitly requested ephemeral auth, there is no persisted fallback.
    if auth_credentials_store_mode == AuthCredentialsStoreMode::Ephemeral {
        return Ok(None);
    }

    // Fall back to managed Praxis auth first, then inherit Codex auth as externally
    // managed tokens so Praxis never rotates Codex's single-use refresh token independently.
    let loaded = match load_persistent_auth_with_origin(
        praxis_home.to_path_buf(),
        auth_credentials_store_mode,
    )? {
        Some(loaded) => loaded,
        None => return Ok(None),
    };

    let auth_dot_json = match loaded.origin {
        LoadedAuthOrigin::Praxis => loaded.auth,
        LoadedAuthOrigin::InheritedCodex => inherited_codex_auth_as_external_tokens(loaded.auth),
    };
    let storage_mode = match loaded.origin {
        LoadedAuthOrigin::Praxis => auth_credentials_store_mode,
        LoadedAuthOrigin::InheritedCodex => AuthCredentialsStoreMode::Ephemeral,
    };

    let auth = build_auth(auth_dot_json, storage_mode)?;
    Ok(Some(auth))
}

fn inherited_codex_auth_as_external_tokens(mut auth_dot_json: AuthDotJson) -> AuthDotJson {
    if auth_dot_json.resolved_mode() == ApiAuthMode::Chatgpt {
        auth_dot_json.auth_mode = Some(ApiAuthMode::ChatgptAuthTokens);
        if let Some(tokens) = auth_dot_json.tokens.as_mut() {
            tokens.refresh_token.clear();
        }
    }
    auth_dot_json
}

// Persist refreshed tokens into auth storage and update last_refresh.
pub(super) fn persist_tokens(
    storage: &Arc<dyn AuthStorageBackend>,
    id_token: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
) -> std::io::Result<AuthDotJson> {
    let mut auth_dot_json = storage
        .load()?
        .ok_or(std::io::Error::other("Token data is not available."))?;

    let tokens = auth_dot_json.tokens.get_or_insert_with(TokenData::default);
    if let Some(id_token) = id_token {
        tokens.id_token = parse_chatgpt_jwt_claims(&id_token).map_err(std::io::Error::other)?;
    }
    if let Some(access_token) = access_token {
        tokens.access_token = access_token;
    }
    if let Some(refresh_token) = refresh_token {
        tokens.refresh_token = refresh_token;
    }
    auth_dot_json.last_refresh = Some(Utc::now());
    storage.save(&auth_dot_json)?;
    Ok(auth_dot_json)
}

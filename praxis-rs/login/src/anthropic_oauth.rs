use praxis_keyring_store::DefaultKeyringStore;
use praxis_keyring_store::KeyringStore;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tiny_http::Header;
use tiny_http::Response;
use tiny_http::Server;
use zeroize::Zeroizing;

use crate::provider_api_key::provider_credential_keyring_account;

const KEYRING_SERVICE: &str = "Praxis Provider OAuth";
const CREDENTIAL_ID: &str = "model-provider-oauth/v1/anthropic";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const MAX_CREDENTIAL_FILE_BYTES: u64 = 64 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;
const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const REDIRECT_URI: &str = "http://localhost:53692/callback";
const SCOPES: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

pub struct AnthropicOauthAccessToken(Zeroizing<String>);

impl AnthropicOauthAccessToken {
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for AnthropicOauthAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AnthropicOauthAccessToken([REDACTED])")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnthropicOauthError {
    #[error("failed to locate the local Claude credential file")]
    HomeUnavailable,
    #[error("the local Claude credential file is too large")]
    SourceTooLarge,
    #[error("failed to read the local Claude credential file")]
    SourceRead,
    #[error("the local Claude credential file has an unsupported schema")]
    SourceSchema,
    #[error("the Anthropic OAuth credential is invalid")]
    InvalidCredential,
    #[error("failed to access the Praxis OAuth credential store")]
    CredentialStore,
    #[error("Anthropic OAuth refresh failed with HTTP status {0}")]
    RefreshStatus(StatusCode),
    #[error("Anthropic OAuth refresh transport failed")]
    RefreshTransport,
    #[error("Anthropic OAuth refresh returned an invalid response")]
    RefreshResponse,
    #[error(
        "the imported Claude login is restricted to Claude Code; run `/login anthropic` to authorize Praxis"
    )]
    MissingThirdPartyScope,
    #[error("failed to start the Anthropic OAuth callback server")]
    CallbackServer,
    #[error("Anthropic OAuth login timed out or was canceled")]
    LoginCanceled,
    #[error("Anthropic OAuth callback validation failed")]
    InvalidCallback,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAnthropicOauthCredential {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token_expires_at: Option<i64>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subscription_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rate_limit_tier: Option<String>,
    source: String,
}

impl fmt::Debug for StoredAnthropicOauthCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredAnthropicOauthCredential")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("refresh_token_expires_at", &self.refresh_token_expires_at)
            .field("scopes", &self.scopes)
            .field("subscription_type", &self.subscription_type)
            .field("rate_limit_tier", &self.rate_limit_tier)
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentialFile {
    claude_ai_oauth: ClaudeOauthBundle,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOauthBundle {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    #[serde(default)]
    refresh_token_expires_at: Option<i64>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    subscription_type: Option<String>,
    #[serde(default)]
    rate_limit_tier: Option<String>,
}

#[derive(Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

pub async fn login_anthropic_oauth(
    praxis_home: &Path,
) -> Result<AnthropicOauthAccessToken, AnthropicOauthError> {
    let pkce = crate::pkce::generate_pkce();
    let state = pkce.code_verifier.clone();
    let server =
        Server::http("127.0.0.1:53692").map_err(|_| AnthropicOauthError::CallbackServer)?;
    let mut authorize =
        url::Url::parse(AUTHORIZE_URL).map_err(|_| AnthropicOauthError::CallbackServer)?;
    authorize
        .query_pairs_mut()
        .append_pair("code", "true")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    webbrowser::open(authorize.as_str()).map_err(|_| AnthropicOauthError::CallbackServer)?;

    let request = tokio::time::timeout(
        Duration::from_secs(10 * 60),
        tokio::task::spawn_blocking(move || server.recv()),
    )
    .await
    .map_err(|_| AnthropicOauthError::LoginCanceled)?
    .map_err(|_| AnthropicOauthError::CallbackServer)?
    .map_err(|_| AnthropicOauthError::CallbackServer)?;
    let callback = url::Url::parse(&format!("http://localhost{}", request.url()))
        .map_err(|_| AnthropicOauthError::InvalidCallback)?;
    let code = callback
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()))
        .ok_or(AnthropicOauthError::InvalidCallback)?;
    let callback_state = callback
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
        .ok_or(AnthropicOauthError::InvalidCallback)?;
    if callback.path() != "/callback" || callback_state != state {
        let _ = request.respond(Response::from_string("Anthropic OAuth callback rejected."));
        return Err(AnthropicOauthError::InvalidCallback);
    }
    let content_type = Header::from_bytes("Content-Type", "text/html; charset=utf-8")
        .map_err(|_| AnthropicOauthError::CallbackServer)?;
    let _ = request.respond(
        Response::from_string(
            "<!doctype html><meta charset=utf-8><title>Praxis</title><p>Claude authentication completed. You can close this window.</p>",
        )
        .with_header(content_type),
    );

    let credential = exchange_authorization_code(&code, &state, &pkce.code_verifier).await?;
    save_stored(praxis_home, &credential)?;
    Ok(AnthropicOauthAccessToken(Zeroizing::new(
        credential.access_token,
    )))
}

pub async fn load_import_and_refresh_anthropic_oauth(
    praxis_home: &Path,
) -> Result<Option<AnthropicOauthAccessToken>, AnthropicOauthError> {
    if let Some(token) = oauth_token_from_environment()? {
        return Ok(Some(token));
    }

    let mut credential = match load_stored(praxis_home)? {
        Some(credential) => credential,
        None => match import_claude_credential()? {
            Some(credential) => {
                save_stored(praxis_home, &credential)?;
                credential
            }
            None => return Ok(None),
        },
    };

    validate_credential(&credential)?;
    if !credential
        .scopes
        .iter()
        .any(|scope| scope == "org:create_api_key")
    {
        return Err(AnthropicOauthError::MissingThirdPartyScope);
    }
    if credential.expires_at <= now_ms().saturating_add(REFRESH_SKEW_MS) {
        credential = refresh_credential(&credential).await?;
        save_stored(praxis_home, &credential)?;
    }
    Ok(Some(AnthropicOauthAccessToken(Zeroizing::new(
        credential.access_token,
    ))))
}

pub fn has_anthropic_oauth(praxis_home: &Path) -> bool {
    if std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
        || std::env::var("ANTHROPIC_OAUTH_TOKEN")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return true;
    }
    load_stored(praxis_home)
        .ok()
        .flatten()
        .is_some_and(|credential| {
            credential
                .scopes
                .iter()
                .any(|scope| scope == "org:create_api_key")
        })
}

fn oauth_token_from_environment() -> Result<Option<AnthropicOauthAccessToken>, AnthropicOauthError>
{
    for name in ["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_OAUTH_TOKEN"] {
        if let Ok(value) = std::env::var(name)
            && !value.trim().is_empty()
        {
            validate_token(&value)?;
            return Ok(Some(AnthropicOauthAccessToken(Zeroizing::new(value))));
        }
    }
    Ok(None)
}

fn import_claude_credential() -> Result<Option<StoredAnthropicOauthCredential>, AnthropicOauthError>
{
    let Some(path) = claude_credential_path() else {
        return Ok(None);
    };
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AnthropicOauthError::SourceRead),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    if metadata.len() > MAX_CREDENTIAL_FILE_BYTES {
        return Err(AnthropicOauthError::SourceTooLarge);
    }
    let source =
        Zeroizing::new(fs::read_to_string(&path).map_err(|_| AnthropicOauthError::SourceRead)?);
    let parsed: ClaudeCredentialFile =
        serde_json::from_str(source.as_str()).map_err(|_| AnthropicOauthError::SourceSchema)?;
    let credential = StoredAnthropicOauthCredential {
        access_token: parsed.claude_ai_oauth.access_token,
        refresh_token: parsed.claude_ai_oauth.refresh_token,
        expires_at: parsed.claude_ai_oauth.expires_at,
        refresh_token_expires_at: parsed.claude_ai_oauth.refresh_token_expires_at,
        scopes: parsed.claude_ai_oauth.scopes,
        subscription_type: parsed.claude_ai_oauth.subscription_type,
        rate_limit_tier: parsed.claude_ai_oauth.rate_limit_tier,
        source: "claude-code-import".to_string(),
    };
    validate_credential(&credential)?;
    Ok(Some(credential))
}

fn claude_credential_path() -> Option<PathBuf> {
    if let Some(config_dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(config_dir).join(".credentials.json"));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".claude")
            .join(".credentials.json"),
    )
}

fn load_stored(
    praxis_home: &Path,
) -> Result<Option<StoredAnthropicOauthCredential>, AnthropicOauthError> {
    let account = provider_credential_keyring_account(praxis_home, CREDENTIAL_ID)
        .map_err(|_| AnthropicOauthError::CredentialStore)?;
    let Some(serialized) = DefaultKeyringStore
        .load(KEYRING_SERVICE, &account)
        .map_err(|_| AnthropicOauthError::CredentialStore)?
    else {
        return Ok(None);
    };
    let serialized = Zeroizing::new(serialized);
    serde_json::from_str(serialized.as_str())
        .map(Some)
        .map_err(|_| AnthropicOauthError::CredentialStore)
}

fn save_stored(
    praxis_home: &Path,
    credential: &StoredAnthropicOauthCredential,
) -> Result<(), AnthropicOauthError> {
    let account = provider_credential_keyring_account(praxis_home, CREDENTIAL_ID)
        .map_err(|_| AnthropicOauthError::CredentialStore)?;
    let serialized = Zeroizing::new(
        serde_json::to_string(credential).map_err(|_| AnthropicOauthError::CredentialStore)?,
    );
    DefaultKeyringStore
        .save(KEYRING_SERVICE, &account, serialized.as_str())
        .map_err(|_| AnthropicOauthError::CredentialStore)
}

async fn refresh_credential(
    credential: &StoredAnthropicOauthCredential,
) -> Result<StoredAnthropicOauthCredential, AnthropicOauthError> {
    if credential
        .refresh_token_expires_at
        .is_some_and(|expires_at| expires_at <= now_ms())
    {
        return Err(AnthropicOauthError::InvalidCredential);
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AnthropicOauthError::RefreshTransport)?;
    let response = client
        .post(TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "client_id": CLIENT_ID,
            "refresh_token": credential.refresh_token,
        }))
        .send()
        .await
        .map_err(|_| AnthropicOauthError::RefreshTransport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(AnthropicOauthError::RefreshStatus(status));
    }
    let refreshed: TokenRefreshResponse = response
        .json()
        .await
        .map_err(|_| AnthropicOauthError::RefreshResponse)?;
    let next = StoredAnthropicOauthCredential {
        access_token: refreshed.access_token,
        refresh_token: refreshed.refresh_token,
        expires_at: now_ms().saturating_add(refreshed.expires_in.saturating_mul(1000)),
        refresh_token_expires_at: credential.refresh_token_expires_at,
        scopes: credential.scopes.clone(),
        subscription_type: credential.subscription_type.clone(),
        rate_limit_tier: credential.rate_limit_tier.clone(),
        source: credential.source.clone(),
    };
    validate_credential(&next)?;
    Ok(next)
}

async fn exchange_authorization_code(
    code: &str,
    state: &str,
    verifier: &str,
) -> Result<StoredAnthropicOauthCredential, AnthropicOauthError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AnthropicOauthError::RefreshTransport)?;
    let response = client
        .post(TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "client_id": CLIENT_ID,
            "code": code,
            "state": state,
            "redirect_uri": REDIRECT_URI,
            "code_verifier": verifier,
        }))
        .send()
        .await
        .map_err(|_| AnthropicOauthError::RefreshTransport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(AnthropicOauthError::RefreshStatus(status));
    }
    let token: TokenRefreshResponse = response
        .json()
        .await
        .map_err(|_| AnthropicOauthError::RefreshResponse)?;
    let credential = StoredAnthropicOauthCredential {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now_ms().saturating_add(token.expires_in.saturating_mul(1000)),
        refresh_token_expires_at: None,
        scopes: SCOPES.split_whitespace().map(str::to_string).collect(),
        subscription_type: None,
        rate_limit_tier: None,
        source: "praxis-oauth".to_string(),
    };
    validate_credential(&credential)?;
    Ok(credential)
}

fn validate_credential(
    credential: &StoredAnthropicOauthCredential,
) -> Result<(), AnthropicOauthError> {
    validate_token(&credential.access_token)?;
    validate_token(&credential.refresh_token)?;
    if credential.expires_at <= 0 {
        return Err(AnthropicOauthError::InvalidCredential);
    }
    Ok(())
}

fn validate_token(token: &str) -> Result<(), AnthropicOauthError> {
    if token.is_empty()
        || token.len() > MAX_TOKEN_BYTES
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(AnthropicOauthError::InvalidCredential);
    }
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

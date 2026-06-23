pub(super) use crate::cache::{
    CloudRequirementsCacheFile, CloudRequirementsCacheSignedPayload, cache_payload_bytes,
    sign_cache_payload,
};
pub(super) use crate::constants::{
    CLOUD_REQUIREMENTS_AUTH_RECOVERY_FAILED_MESSAGE, CLOUD_REQUIREMENTS_CACHE_FILENAME,
    CLOUD_REQUIREMENTS_MAX_ATTEMPTS, CLOUD_REQUIREMENTS_TIMEOUT,
};
pub(super) use crate::fetcher::{FetchAttemptError, RequirementsFetcher, RetryableFailureKind};
pub(super) use crate::parsing::{
    bundle_from_requirements_contents, parse_cloud_requirements, requirements_from_bundle,
};
pub(super) use crate::service::CloudRequirementsService;
pub(super) use base64::Engine;
pub(super) use base64::engine::general_purpose::URL_SAFE_NO_PAD;
pub(super) use chrono::{Duration as ChronoDuration, Utc};
pub(super) use praxis_core::config_loader::ConfigRequirementsToml;
pub(super) use praxis_login::AuthCredentialsStoreMode;
pub(super) use praxis_login::{AuthManager, OpenAiAccountAuth};
pub(super) use praxis_protocol::protocol::AskForApproval;
pub(super) use pretty_assertions::assert_eq;
pub(super) use serde_json::json;
pub(super) use std::collections::BTreeMap;
pub(super) use std::collections::VecDeque;
pub(super) use std::future::pending;
pub(super) use std::path::Path;
pub(super) use std::sync::Arc;
pub(super) use std::sync::atomic::AtomicUsize;
pub(super) use std::sync::atomic::Ordering;
pub(super) use std::time::Duration;
pub(super) use tempfile::TempDir;
pub(super) use tempfile::tempdir;

pub(super) fn write_auth_json(praxis_home: &Path, value: serde_json::Value) -> std::io::Result<()> {
    std::fs::write(
        praxis_home.join("auth.json"),
        serde_json::to_string(&value)?,
    )?;
    Ok(())
}

pub(super) fn auth_manager_with_api_key() -> Arc<AuthManager> {
    let tmp = tempdir().expect("tempdir");
    let auth_json = json!({
        "OPENAI_API_KEY": "sk-test-key",
        "tokens": null,
        "last_refresh": null,
    });
    write_auth_json(tmp.path(), auth_json).expect("write auth");
    Arc::new(AuthManager::new(
        tmp.path().to_path_buf(),
        /*enable_praxis_api_key_env*/ false,
        AuthCredentialsStoreMode::File,
    ))
}

pub(super) fn auth_manager_with_plan_and_identity(
    plan_type: &str,
    chatgpt_user_id: Option<&str>,
    account_id: Option<&str>,
) -> Arc<AuthManager> {
    let tmp = tempdir().expect("tempdir");
    write_auth_json(
        tmp.path(),
        chatgpt_auth_json(
            plan_type,
            chatgpt_user_id,
            account_id,
            "test-access-token",
            "test-refresh-token",
        ),
    )
    .expect("write auth");
    Arc::new(AuthManager::new(
        tmp.path().to_path_buf(),
        /*enable_praxis_api_key_env*/ false,
        AuthCredentialsStoreMode::File,
    ))
}

pub(super) fn chatgpt_auth_json(
    plan_type: &str,
    chatgpt_user_id: Option<&str>,
    account_id: Option<&str>,
    access_token: &str,
    refresh_token: &str,
) -> serde_json::Value {
    chatgpt_auth_json_with_last_refresh(
        plan_type,
        chatgpt_user_id,
        account_id,
        access_token,
        refresh_token,
        "2025-01-01T00:00:00Z",
    )
}

pub(super) fn chatgpt_auth_json_with_last_refresh(
    plan_type: &str,
    chatgpt_user_id: Option<&str>,
    account_id: Option<&str>,
    access_token: &str,
    refresh_token: &str,
    last_refresh: &str,
) -> serde_json::Value {
    chatgpt_auth_json_with_mode(
        plan_type,
        chatgpt_user_id,
        account_id,
        access_token,
        refresh_token,
        last_refresh,
        /*auth_mode*/ None,
    )
}

pub(super) fn chatgpt_auth_json_with_mode(
    plan_type: &str,
    chatgpt_user_id: Option<&str>,
    account_id: Option<&str>,
    access_token: &str,
    refresh_token: &str,
    last_refresh: &str,
    auth_mode: Option<&str>,
) -> serde_json::Value {
    let header = json!({ "alg": "none", "typ": "JWT" });
    let auth_payload = json!({
        "chatgpt_plan_type": plan_type,
        "chatgpt_user_id": chatgpt_user_id,
        "user_id": chatgpt_user_id,
    });
    let payload = json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": auth_payload,
    });
    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).expect("header"));
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("payload"));
    let signature_b64 = URL_SAFE_NO_PAD.encode(b"sig");
    let fake_jwt = format!("{header_b64}.{payload_b64}.{signature_b64}");

    let mut auth_json = json!({
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": fake_jwt,
            "access_token": access_token,
            "refresh_token": refresh_token,
            "account_id": account_id,
        },
        "last_refresh": last_refresh,
    });
    if let Some(auth_mode) = auth_mode {
        auth_json["auth_mode"] = serde_json::Value::String(auth_mode.to_string());
    }
    auth_json
}

pub(super) struct ManagedAuthContext {
    pub(super) _home: TempDir,
    pub(super) manager: Arc<AuthManager>,
}

pub(super) fn managed_auth_context(
    plan_type: &str,
    chatgpt_user_id: Option<&str>,
    account_id: Option<&str>,
    access_token: &str,
    refresh_token: &str,
) -> ManagedAuthContext {
    let home = tempdir().expect("tempdir");
    write_auth_json(
        home.path(),
        chatgpt_auth_json(
            plan_type,
            chatgpt_user_id,
            account_id,
            access_token,
            refresh_token,
        ),
    )
    .expect("write auth");
    ManagedAuthContext {
        manager: Arc::new(AuthManager::new(
            home.path().to_path_buf(),
            /*enable_praxis_api_key_env*/ false,
            AuthCredentialsStoreMode::File,
        )),
        _home: home,
    }
}

pub(super) fn auth_manager_with_plan(plan_type: &str) -> Arc<AuthManager> {
    auth_manager_with_plan_and_identity(plan_type, Some("user-12345"), Some("account-12345"))
}

pub(super) fn parse_for_fetch(contents: Option<&str>) -> Option<ConfigRequirementsToml> {
    contents.and_then(|contents| parse_cloud_requirements(contents).ok().flatten())
}

pub(super) fn request_error() -> FetchAttemptError {
    FetchAttemptError::Retryable(RetryableFailureKind::Request { status_code: None })
}

pub(super) struct StaticFetcher {
    pub(super) contents: Option<String>,
}

#[async_trait::async_trait]
impl RequirementsFetcher for StaticFetcher {
    async fn fetch_requirements(
        &self,
        _auth: &OpenAiAccountAuth,
    ) -> Result<Option<String>, FetchAttemptError> {
        Ok(self.contents.clone())
    }
}

pub(super) struct PendingFetcher;

#[async_trait::async_trait]
impl RequirementsFetcher for PendingFetcher {
    async fn fetch_requirements(
        &self,
        _auth: &OpenAiAccountAuth,
    ) -> Result<Option<String>, FetchAttemptError> {
        pending::<()>().await;
        Ok(None)
    }
}

pub(super) struct SequenceFetcher {
    pub(super) responses: tokio::sync::Mutex<VecDeque<Result<Option<String>, FetchAttemptError>>>,
    pub(super) request_count: AtomicUsize,
}

impl SequenceFetcher {
    pub(super) fn new(responses: Vec<Result<Option<String>, FetchAttemptError>>) -> Self {
        Self {
            responses: tokio::sync::Mutex::new(VecDeque::from(responses)),
            request_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl RequirementsFetcher for SequenceFetcher {
    async fn fetch_requirements(
        &self,
        _auth: &OpenAiAccountAuth,
    ) -> Result<Option<String>, FetchAttemptError> {
        self.request_count.fetch_add(1, Ordering::SeqCst);
        let mut responses = self.responses.lock().await;
        responses.pop_front().unwrap_or(Ok(None))
    }
}

pub(super) struct TokenFetcher {
    pub(super) expected_token: String,
    pub(super) contents: String,
    pub(super) request_count: AtomicUsize,
}

#[async_trait::async_trait]
impl RequirementsFetcher for TokenFetcher {
    async fn fetch_requirements(
        &self,
        auth: &OpenAiAccountAuth,
    ) -> Result<Option<String>, FetchAttemptError> {
        self.request_count.fetch_add(1, Ordering::SeqCst);
        if matches!(
            auth.get_token().as_deref(),
            Ok(token) if token == self.expected_token.as_str()
        ) {
            Ok(Some(self.contents.clone()))
        } else {
            Err(FetchAttemptError::Unauthorized {
                status_code: Some(401),
                message: "GET /config/requirements failed: 401".to_string(),
            })
        }
    }
}

pub(super) struct UnauthorizedFetcher {
    pub(super) message: String,
    pub(super) request_count: AtomicUsize,
}

#[async_trait::async_trait]
impl RequirementsFetcher for UnauthorizedFetcher {
    async fn fetch_requirements(
        &self,
        _auth: &OpenAiAccountAuth,
    ) -> Result<Option<String>, FetchAttemptError> {
        self.request_count.fetch_add(1, Ordering::SeqCst);
        Err(FetchAttemptError::Unauthorized {
            status_code: Some(401),
            message: self.message.clone(),
        })
    }
}

use crate::constants::{
    CLOUD_REQUIREMENTS_CACHE_FILENAME, CLOUD_REQUIREMENTS_CACHE_READ_HMAC_KEYS,
    CLOUD_REQUIREMENTS_CACHE_TTL, CLOUD_REQUIREMENTS_CACHE_WRITE_HMAC_KEY,
};
use crate::parsing::bundle_from_requirements_contents;
#[cfg(test)]
use crate::parsing::requirements_from_bundle;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hmac::{Hmac, Mac};
use praxis_core::config_loader::CloudConfigBundle;
#[cfg(test)]
use praxis_core::config_loader::ConfigRequirementsToml;
use praxis_login::OpenAiAccountAuth;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug)]
pub(crate) struct CloudRequirementsCache {
    path: PathBuf,
}

impl CloudRequirementsCache {
    pub(crate) fn new(praxis_home: PathBuf) -> Self {
        Self {
            path: praxis_home.join(CLOUD_REQUIREMENTS_CACHE_FILENAME),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) async fn load(
        &self,
        chatgpt_user_id: Option<&str>,
        account_id: Option<&str>,
    ) -> Result<CloudRequirementsCacheSignedPayload, CacheLoadStatus> {
        let (Some(chatgpt_user_id), Some(account_id)) = (chatgpt_user_id, account_id) else {
            return Err(CacheLoadStatus::AuthIdentityIncomplete);
        };

        let bytes = match fs::read(&self.path).await {
            Ok(bytes) => bytes,
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    return Err(CacheLoadStatus::CacheReadFailed(err.to_string()));
                }
                return Err(CacheLoadStatus::CacheFileNotFound);
            }
        };

        let cache_file: CloudRequirementsCacheFile = match serde_json::from_slice(&bytes) {
            Ok(cache_file) => cache_file,
            Err(err) => return Err(CacheLoadStatus::CacheParseFailed(err.to_string())),
        };
        let payload_bytes = match cache_payload_bytes(&cache_file.signed_payload) {
            Some(payload_bytes) => payload_bytes,
            None => {
                return Err(CacheLoadStatus::CacheParseFailed(
                    "failed to serialize cache payload".to_string(),
                ));
            }
        };
        if !verify_cache_signature(&payload_bytes, &cache_file.signature) {
            return Err(CacheLoadStatus::CacheSignatureInvalid);
        }

        let (Some(cached_chatgpt_user_id), Some(cached_account_id)) = (
            cache_file.signed_payload.chatgpt_user_id.as_deref(),
            cache_file.signed_payload.account_id.as_deref(),
        ) else {
            return Err(CacheLoadStatus::CacheIdentityIncomplete);
        };

        if cached_chatgpt_user_id != chatgpt_user_id || cached_account_id != account_id {
            return Err(CacheLoadStatus::CacheIdentityMismatch);
        }

        if cache_file.signed_payload.expires_at <= Utc::now() {
            return Err(CacheLoadStatus::CacheExpired);
        }

        Ok(cache_file.signed_payload)
    }

    pub(crate) fn log_load_status(&self, status: &CacheLoadStatus) {
        if matches!(status, CacheLoadStatus::CacheFileNotFound) {
            return;
        }

        let warn = matches!(
            status,
            CacheLoadStatus::CacheReadFailed(_)
                | CacheLoadStatus::CacheParseFailed(_)
                | CacheLoadStatus::CacheSignatureInvalid
        );

        if warn {
            tracing::warn!(path = %self.path.display(), "{status}");
        } else {
            tracing::info!(path = %self.path.display(), "{status}");
        }
    }

    pub(crate) async fn save(
        &self,
        chatgpt_user_id: Option<String>,
        account_id: Option<String>,
        contents: Option<String>,
    ) -> Result<(), CloudRequirementsError> {
        let now = Utc::now();
        let expires_at = now
            .checked_add_signed(
                ChronoDuration::from_std(CLOUD_REQUIREMENTS_CACHE_TTL)
                    .map_err(|_| CloudRequirementsError::CacheWrite)?,
            )
            .ok_or(CloudRequirementsError::CacheWrite)?;
        let signed_payload = CloudRequirementsCacheSignedPayload {
            cached_at: now,
            expires_at,
            chatgpt_user_id,
            account_id,
            contents,
        };
        let payload_bytes =
            cache_payload_bytes(&signed_payload).ok_or(CloudRequirementsError::CacheWrite)?;
        let serialized = serde_json::to_vec_pretty(&CloudRequirementsCacheFile {
            signature: sign_cache_payload(&payload_bytes)
                .ok_or(CloudRequirementsError::CacheWrite)?,
            signed_payload,
        })
        .map_err(|_| CloudRequirementsError::CacheWrite)?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|_| CloudRequirementsError::CacheWrite)?;
        }

        fs::write(&self.path, serialized)
            .await
            .map_err(|_| CloudRequirementsError::CacheWrite)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub(crate) enum CacheLoadStatus {
    #[error("Skipping cloud requirements cache read because auth identity is incomplete.")]
    AuthIdentityIncomplete,
    #[error("Cloud requirements cache file not found.")]
    CacheFileNotFound,
    #[error("Failed to read cloud requirements cache: {0}.")]
    CacheReadFailed(String),
    #[error("Failed to parse cloud requirements cache: {0}.")]
    CacheParseFailed(String),
    #[error("Cloud requirements cache failed signature verification.")]
    CacheSignatureInvalid,
    #[error("Ignoring cloud requirements cache because cached identity is incomplete.")]
    CacheIdentityIncomplete,
    #[error("Ignoring cloud requirements cache for different auth identity.")]
    CacheIdentityMismatch,
    #[error("Cloud requirements cache expired.")]
    CacheExpired,
}

#[derive(Debug, Error)]
pub(crate) enum CloudRequirementsError {
    #[error("failed to write cloud requirements cache")]
    CacheWrite,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CloudRequirementsCacheFile {
    pub(crate) signed_payload: CloudRequirementsCacheSignedPayload,
    pub(crate) signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CloudRequirementsCacheSignedPayload {
    pub(crate) cached_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) chatgpt_user_id: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) contents: Option<String>,
}

impl CloudRequirementsCacheSignedPayload {
    pub(crate) fn bundle(&self) -> Result<Option<CloudConfigBundle>, toml::de::Error> {
        bundle_from_requirements_contents(self.contents.clone())
    }

    #[cfg(test)]
    pub(crate) fn requirements(&self) -> Option<ConfigRequirementsToml> {
        self.bundle()
            .ok()
            .flatten()
            .and_then(|bundle| requirements_from_bundle(&bundle).ok().flatten())
    }
}

pub(crate) fn sign_cache_payload(payload_bytes: &[u8]) -> Option<String> {
    let mut mac = HmacSha256::new_from_slice(CLOUD_REQUIREMENTS_CACHE_WRITE_HMAC_KEY).ok()?;
    mac.update(payload_bytes);
    let signature = mac.finalize().into_bytes();
    Some(BASE64_STANDARD.encode(signature))
}

fn verify_cache_signature_with_key(
    payload_bytes: &[u8],
    signature_bytes: &[u8],
    key: &[u8],
) -> bool {
    let mut mac = match HmacSha256::new_from_slice(key) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(payload_bytes);
    mac.verify_slice(signature_bytes).is_ok()
}

fn verify_cache_signature(payload_bytes: &[u8], signature: &str) -> bool {
    let signature_bytes = match BASE64_STANDARD.decode(signature) {
        Ok(signature_bytes) => signature_bytes,
        Err(_) => return false,
    };

    CLOUD_REQUIREMENTS_CACHE_READ_HMAC_KEYS
        .iter()
        .any(|key| verify_cache_signature_with_key(payload_bytes, &signature_bytes, key))
}

pub(crate) fn auth_identity(auth: &OpenAiAccountAuth) -> (Option<String>, Option<String>) {
    let token_data = auth.get_token_data().ok();
    let chatgpt_user_id = token_data
        .as_ref()
        .and_then(|token_data| token_data.id_token.chatgpt_user_id.as_deref())
        .map(str::to_owned);
    let account_id = auth.get_account_id();
    (chatgpt_user_id, account_id)
}

pub(crate) fn cache_payload_bytes(
    payload: &CloudRequirementsCacheSignedPayload,
) -> Option<Vec<u8>> {
    serde_json::to_vec(&payload).ok()
}

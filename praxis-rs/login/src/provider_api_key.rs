use praxis_keyring_store::DefaultKeyringStore;
use praxis_keyring_store::KeyringStore;
use sha2::Digest;
use sha2::Sha256;
use std::fmt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use zeroize::Zeroizing;

const KEYRING_SERVICE: &str = "Praxis Provider API Keys";
const KEYRING_ACCOUNT_VERSION: &str = "v1";
const MODEL_PROVIDER_CREDENTIAL_ID_PREFIX: &str = "model-provider/v1/";
const MAX_CREDENTIAL_ID_BYTES: usize = 256;
const MAX_API_KEY_BYTES: usize = 2048;

/// A redacted provider API key whose owned memory is zeroed on drop.
pub struct ProviderApiKey(Zeroizing<String>);

impl ProviderApiKey {
    /// Validate and wrap a provider API key without exposing it through `Debug`.
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderApiKeyError> {
        let value = Zeroizing::new(value.into());
        validate_api_key(value.as_str())?;
        Ok(Self(value))
    }

    /// Explicitly expose the secret for an authenticated provider request.
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ProviderApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProviderApiKey([REDACTED])")
    }
}

/// Errors from the provider API key credential boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProviderApiKeyError {
    #[error("praxis_home must not be empty")]
    InvalidPraxisHome,
    #[error("credential_id must be a non-empty printable ASCII identifier of at most 256 bytes")]
    InvalidCredentialId,
    #[error("provider API key must be non-empty printable ASCII of at most 2048 bytes")]
    InvalidApiKey,
    #[error("the provider API key stored in the OS credential store is invalid")]
    InvalidStoredApiKey,
    #[error("failed to resolve praxis_home for provider credential isolation")]
    ResolvePraxisHome,
    #[error("failed to save provider API key in the OS credential store")]
    SaveFailed,
    #[error("failed to load provider API key from the OS credential store")]
    LoadFailed,
    #[error("failed to delete provider API key from the OS credential store")]
    DeleteFailed,
}

/// Map a provider ID to the stable, versioned credential ID used by provider API key storage.
pub fn provider_api_key_credential_id(provider_id: &str) -> Result<String, ProviderApiKeyError> {
    if provider_id != provider_id.trim() || provider_id.is_empty() {
        return Err(ProviderApiKeyError::InvalidCredentialId);
    }
    let credential_id = format!("{MODEL_PROVIDER_CREDENTIAL_ID_PREFIX}{provider_id}");
    validate_credential_id(&credential_id)?;
    Ok(credential_id)
}

/// Save an API key exclusively in the OS credential store using a stable credential ID, preferably from `provider_api_key_credential_id`.
pub fn save_provider_api_key(
    praxis_home: &Path,
    credential_id: &str,
    api_key: &str,
) -> Result<(), ProviderApiKeyError> {
    save_provider_api_key_with_store(praxis_home, credential_id, api_key, &DefaultKeyringStore)
}

/// Load a provider API key by its stable provider-scoped credential ID.
pub fn load_provider_api_key(
    praxis_home: &Path,
    credential_id: &str,
) -> Result<Option<ProviderApiKey>, ProviderApiKeyError> {
    load_provider_api_key_with_store(praxis_home, credential_id, &DefaultKeyringStore)
}

/// Delete a provider API key by its stable provider-scoped credential ID.
pub fn delete_provider_api_key(
    praxis_home: &Path,
    credential_id: &str,
) -> Result<bool, ProviderApiKeyError> {
    delete_provider_api_key_with_store(praxis_home, credential_id, &DefaultKeyringStore)
}

fn save_provider_api_key_with_store(
    praxis_home: &Path,
    credential_id: &str,
    api_key: &str,
    keyring_store: &dyn KeyringStore,
) -> Result<(), ProviderApiKeyError> {
    let account = provider_credential_keyring_account(praxis_home, credential_id)?;
    let api_key = ProviderApiKey::new(api_key)?;
    keyring_store
        .save(KEYRING_SERVICE, &account, api_key.expose_secret())
        .map_err(|_| ProviderApiKeyError::SaveFailed)
}

fn load_provider_api_key_with_store(
    praxis_home: &Path,
    credential_id: &str,
    keyring_store: &dyn KeyringStore,
) -> Result<Option<ProviderApiKey>, ProviderApiKeyError> {
    let account = provider_credential_keyring_account(praxis_home, credential_id)?;
    let value = keyring_store
        .load(KEYRING_SERVICE, &account)
        .map_err(|_| ProviderApiKeyError::LoadFailed)?;
    value
        .map(|value| {
            ProviderApiKey::new(value).map_err(|_| ProviderApiKeyError::InvalidStoredApiKey)
        })
        .transpose()
}

fn delete_provider_api_key_with_store(
    praxis_home: &Path,
    credential_id: &str,
    keyring_store: &dyn KeyringStore,
) -> Result<bool, ProviderApiKeyError> {
    let account = provider_credential_keyring_account(praxis_home, credential_id)?;
    keyring_store
        .delete(KEYRING_SERVICE, &account)
        .map_err(|_| ProviderApiKeyError::DeleteFailed)
}

pub(crate) fn provider_credential_keyring_account(
    praxis_home: &Path,
    credential_id: &str,
) -> Result<String, ProviderApiKeyError> {
    validate_credential_id(credential_id)?;
    let praxis_home = normalized_absolute_home(praxis_home)?;
    let home_digest = hash_path(&praxis_home);
    let credential_digest = Sha256::digest(credential_id.as_bytes());
    Ok(format!(
        "{KEYRING_ACCOUNT_VERSION}|{home_digest:x}|{credential_digest:x}"
    ))
}

fn normalized_absolute_home(praxis_home: &Path) -> Result<PathBuf, ProviderApiKeyError> {
    if praxis_home.as_os_str().is_empty() {
        return Err(ProviderApiKeyError::InvalidPraxisHome);
    }

    let absolute = if praxis_home.is_absolute() {
        praxis_home.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ProviderApiKeyError::ResolvePraxisHome)?
            .join(praxis_home)
    };
    let normalized = normalize_lexically(&absolute);
    Ok(dunce::canonicalize(&normalized).unwrap_or(normalized))
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(windows)]
fn hash_path(path: &Path) -> sha2::digest::Output<Sha256> {
    use std::os::windows::ffi::OsStrExt;

    let mut hasher = Sha256::new();
    for code_unit in path.as_os_str().encode_wide() {
        hasher.update(code_unit.to_le_bytes());
    }
    hasher.finalize()
}

#[cfg(unix)]
fn hash_path(path: &Path) -> sha2::digest::Output<Sha256> {
    use std::os::unix::ffi::OsStrExt;

    Sha256::digest(path.as_os_str().as_bytes())
}

#[cfg(not(any(unix, windows)))]
fn hash_path(path: &Path) -> sha2::digest::Output<Sha256> {
    Sha256::digest(path.as_os_str().to_string_lossy().as_bytes())
}

fn validate_credential_id(credential_id: &str) -> Result<(), ProviderApiKeyError> {
    if credential_id.is_empty()
        || credential_id.len() > MAX_CREDENTIAL_ID_BYTES
        || !credential_id.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(ProviderApiKeyError::InvalidCredentialId);
    }
    Ok(())
}

fn validate_api_key(api_key: &str) -> Result<(), ProviderApiKeyError> {
    if api_key.is_empty()
        || api_key.len() > MAX_API_KEY_BYTES
        || !api_key.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(ProviderApiKeyError::InvalidApiKey);
    }
    Ok(())
}

#[cfg(test)]
#[path = "provider_api_key_tests.rs"]
mod tests;

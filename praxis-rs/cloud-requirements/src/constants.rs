use std::time::Duration;

pub(crate) const CLOUD_REQUIREMENTS_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const CLOUD_REQUIREMENTS_MAX_ATTEMPTS: usize = 5;
pub(crate) const CLOUD_REQUIREMENTS_CACHE_FILENAME: &str = "cloud-requirements-cache.json";
pub(crate) const CLOUD_REQUIREMENTS_CACHE_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub(crate) const CLOUD_REQUIREMENTS_CACHE_TTL: Duration = Duration::from_secs(30 * 60);
pub(crate) const CLOUD_REQUIREMENTS_FETCH_ATTEMPT_METRIC: &str =
    "praxis.cloud_requirements.fetch_attempt";
pub(crate) const CLOUD_REQUIREMENTS_FETCH_FINAL_METRIC: &str =
    "praxis.cloud_requirements.fetch_final";
pub(crate) const CLOUD_REQUIREMENTS_LOAD_METRIC: &str = "praxis.cloud_requirements.load";
pub(crate) const CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE: &str =
    "failed to load your workspace-managed config";
pub(crate) const CLOUD_REQUIREMENTS_AUTH_RECOVERY_FAILED_MESSAGE: &str = "Your authentication session could not be refreshed automatically. Please log out and sign in again.";
pub(crate) const OPENAI_PRAXIS_REQUIREMENTS_FRAGMENT_ID: &str = "openai-praxis-cloud-requirements";
pub(crate) const OPENAI_PRAXIS_REQUIREMENTS_FRAGMENT_NAME: &str =
    "OpenAI Praxis cloud requirements";
pub(crate) const CLOUD_REQUIREMENTS_CACHE_WRITE_HMAC_KEY: &[u8] =
    b"praxis-cloud-requirements-cache-v3-064f8542-75b4-494c-a294-97d3ce597271";
pub(crate) const CLOUD_REQUIREMENTS_CACHE_READ_HMAC_KEYS: &[&[u8]] =
    &[CLOUD_REQUIREMENTS_CACHE_WRITE_HMAC_KEY];

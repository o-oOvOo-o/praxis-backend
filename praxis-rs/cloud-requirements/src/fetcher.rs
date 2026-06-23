use async_trait::async_trait;
use praxis_backend_client::Client as BackendClient;
use praxis_login::OpenAiAccountAuth;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetryableFailureKind {
    BackendClientInit,
    Request { status_code: Option<u16> },
}

impl RetryableFailureKind {
    pub(crate) fn status_code(self) -> Option<u16> {
        match self {
            Self::BackendClientInit => None,
            Self::Request { status_code } => status_code,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FetchAttemptError {
    Retryable(RetryableFailureKind),
    Unauthorized {
        status_code: Option<u16>,
        message: String,
    },
}

#[async_trait]
pub(crate) trait RequirementsFetcher: Send + Sync {
    /// Returns `Ok(None)` when there are no cloud requirements for the account.
    ///
    /// Returning `Err` indicates cloud requirements could not be fetched.
    async fn fetch_requirements(
        &self,
        auth: &OpenAiAccountAuth,
    ) -> Result<Option<String>, FetchAttemptError>;
}

pub(crate) struct BackendRequirementsFetcher {
    base_url: String,
}

impl BackendRequirementsFetcher {
    pub(crate) fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

#[async_trait]
impl RequirementsFetcher for BackendRequirementsFetcher {
    async fn fetch_requirements(
        &self,
        auth: &OpenAiAccountAuth,
    ) -> Result<Option<String>, FetchAttemptError> {
        let client = BackendClient::from_auth(self.base_url.clone(), auth)
            .inspect_err(|err| {
                tracing::warn!(
                    error = %err,
                    "Failed to construct backend client for cloud requirements"
                );
            })
            .map_err(|_| FetchAttemptError::Retryable(RetryableFailureKind::BackendClientInit))?;

        let response = client
            .get_config_requirements_file()
            .await
            .inspect_err(|err| tracing::warn!(error = %err, "Failed to fetch cloud requirements"))
            .map_err(|err| {
                let status_code = err.status().map(|status| status.as_u16());
                if err.is_unauthorized() {
                    FetchAttemptError::Unauthorized {
                        status_code,
                        message: err.to_string(),
                    }
                } else {
                    FetchAttemptError::Retryable(RetryableFailureKind::Request { status_code })
                }
            })?;

        let Some(contents) = response.contents else {
            tracing::info!(
                "Cloud requirements response missing contents; treating as no requirements"
            );
            return Ok(None);
        };

        Ok(Some(contents))
    }
}

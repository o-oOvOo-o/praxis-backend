use crate::constants::CLOUD_REQUIREMENTS_TIMEOUT;
use crate::fetcher::BackendRequirementsFetcher;
use crate::service::CloudRequirementsService;
use async_trait::async_trait;
use praxis_core::config_loader::{
    CloudConfigBundle, CloudConfigBundleLoadError, CloudConfigBundleLoadErrorCode,
};
use praxis_login::AuthManager;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

#[async_trait]
pub trait ConfigBundleProvider: Send + Sync {
    async fn load_bundle(&self) -> Result<Option<CloudConfigBundle>, CloudConfigBundleLoadError>;
}

#[derive(Clone, Debug, Default)]
pub struct NoopConfigBundleProvider;

#[async_trait]
impl ConfigBundleProvider for NoopConfigBundleProvider {
    async fn load_bundle(&self) -> Result<Option<CloudConfigBundle>, CloudConfigBundleLoadError> {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct OpenAiHostedConfigBundleProvider {
    service: CloudRequirementsService,
}

impl OpenAiHostedConfigBundleProvider {
    pub fn new(
        auth_manager: Arc<AuthManager>,
        chatgpt_base_url: String,
        praxis_home: PathBuf,
    ) -> Self {
        Self {
            service: CloudRequirementsService::new(
                auth_manager,
                Arc::new(BackendRequirementsFetcher::new(chatgpt_base_url)),
                praxis_home,
                CLOUD_REQUIREMENTS_TIMEOUT,
            ),
        }
    }

    pub(crate) fn refresh_service(&self) -> CloudRequirementsService {
        self.service.clone()
    }
}

#[async_trait]
impl ConfigBundleProvider for OpenAiHostedConfigBundleProvider {
    async fn load_bundle(&self) -> Result<Option<CloudConfigBundle>, CloudConfigBundleLoadError> {
        self.service.fetch_bundle_with_timeout().await
    }
}

#[derive(Clone, Debug)]
pub struct LocalFileConfigBundleProvider {
    path: PathBuf,
}

impl LocalFileConfigBundleProvider {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl ConfigBundleProvider for LocalFileConfigBundleProvider {
    async fn load_bundle(&self) -> Result<Option<CloudConfigBundle>, CloudConfigBundleLoadError> {
        let contents = match fs::read_to_string(&self.path).await {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(CloudConfigBundleLoadError::new(
                    CloudConfigBundleLoadErrorCode::RequestFailed,
                    None,
                    format!(
                        "failed to read local cloud config bundle {}: {err}",
                        self.path.display()
                    ),
                ));
            }
        };
        if contents.trim().is_empty() {
            return Ok(None);
        }

        let bundle: CloudConfigBundle = if self
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            serde_json::from_str(&contents).map_err(|err| {
                CloudConfigBundleLoadError::new(
                    CloudConfigBundleLoadErrorCode::InvalidBundle,
                    None,
                    format!(
                        "failed to parse local cloud config bundle {}: {err}",
                        self.path.display()
                    ),
                )
            })?
        } else {
            toml::from_str(&contents).map_err(|err| {
                CloudConfigBundleLoadError::new(
                    CloudConfigBundleLoadErrorCode::InvalidBundle,
                    None,
                    format!(
                        "failed to parse local cloud config bundle {}: {err}",
                        self.path.display()
                    ),
                )
            })?
        };

        Ok(Some(bundle).filter(|bundle| !bundle.is_empty()))
    }
}

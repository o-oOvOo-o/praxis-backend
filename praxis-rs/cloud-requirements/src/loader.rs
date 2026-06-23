use crate::parsing::{cloud_bundle_error_to_requirements_error, requirements_from_bundle_option};
use crate::provider::{ConfigBundleProvider, OpenAiHostedConfigBundleProvider};
use praxis_core::config_loader::{
    CloudConfigBundleLoadError, CloudConfigBundleLoadErrorCode, CloudConfigBundleLoader,
    CloudRequirementsLoadError, CloudRequirementsLoader,
};
use praxis_login::{AuthCredentialsStoreMode, AuthManager};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::task::JoinHandle;

fn refresher_task_slot() -> &'static Mutex<Option<JoinHandle<()>>> {
    static REFRESHER_TASK: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();
    REFRESHER_TASK.get_or_init(|| Mutex::new(None))
}

pub fn cloud_requirements_loader(
    auth_manager: Arc<AuthManager>,
    chatgpt_base_url: String,
    praxis_home: PathBuf,
) -> CloudRequirementsLoader {
    let loader = cloud_config_bundle_loader(auth_manager, chatgpt_base_url, praxis_home);
    CloudRequirementsLoader::new(async move {
        let bundle = loader
            .get()
            .await
            .map_err(cloud_bundle_error_to_requirements_error)?;
        requirements_from_bundle_option(bundle).map_err(|err| {
            tracing::error!(error = %err, "Failed to parse cloud requirements bundle");
            CloudRequirementsLoadError::new(
                praxis_core::config_loader::CloudRequirementsLoadErrorCode::Parse,
                None,
                crate::constants::CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE,
            )
        })
    })
}

pub fn cloud_requirements_loader_for_storage(
    praxis_home: PathBuf,
    enable_praxis_api_key_env: bool,
    credentials_store_mode: AuthCredentialsStoreMode,
    chatgpt_base_url: String,
) -> CloudRequirementsLoader {
    let auth_manager = AuthManager::shared(
        praxis_home.clone(),
        enable_praxis_api_key_env,
        credentials_store_mode,
    );
    cloud_requirements_loader(auth_manager, chatgpt_base_url, praxis_home)
}

pub fn cloud_config_bundle_loader(
    auth_manager: Arc<AuthManager>,
    chatgpt_base_url: String,
    praxis_home: PathBuf,
) -> CloudConfigBundleLoader {
    let provider =
        OpenAiHostedConfigBundleProvider::new(auth_manager, chatgpt_base_url, praxis_home);
    let refresh_service = provider.refresh_service();
    let refresh_task =
        tokio::spawn(async move { refresh_service.refresh_cache_in_background().await });
    let mut refresher_guard = refresher_task_slot().lock().unwrap_or_else(|err| {
        tracing::warn!("cloud requirements refresher task slot was poisoned");
        err.into_inner()
    });
    if let Some(existing_task) = refresher_guard.replace(refresh_task) {
        existing_task.abort();
    }
    cloud_config_bundle_loader_from_provider(Arc::new(provider))
}

pub fn cloud_config_bundle_loader_for_storage(
    praxis_home: PathBuf,
    enable_praxis_api_key_env: bool,
    credentials_store_mode: AuthCredentialsStoreMode,
    chatgpt_base_url: String,
) -> CloudConfigBundleLoader {
    let auth_manager = AuthManager::shared(
        praxis_home.clone(),
        enable_praxis_api_key_env,
        credentials_store_mode,
    );
    cloud_config_bundle_loader(auth_manager, chatgpt_base_url, praxis_home)
}

pub fn cloud_config_bundle_loader_from_provider(
    provider: Arc<dyn ConfigBundleProvider>,
) -> CloudConfigBundleLoader {
    let task = tokio::spawn(async move { provider.load_bundle().await });
    CloudConfigBundleLoader::new(async move {
        task.await.map_err(|err| {
            tracing::error!(error = %err, "Cloud config bundle task failed");
            CloudConfigBundleLoadError::new(
                CloudConfigBundleLoadErrorCode::Internal,
                /*status_code*/ None,
                format!("cloud config bundle load failed: {err}"),
            )
        })?
    })
}

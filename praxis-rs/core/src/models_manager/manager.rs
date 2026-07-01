use super::cache::ModelsCacheManager;
use crate::api_bridge::map_api_error;
use crate::auth_env_telemetry::AuthEnvTelemetry;
use crate::config::Config;
use crate::error::PraxisErr;
use crate::error::Result as CoreResult;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::OPENAI_PROVIDER_ID;
use crate::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use crate::models_manager::collaboration_mode_presets::builtin_collaboration_mode_presets;
use crate::models_manager::model_info;
use crate::plugins::PluginsManager;
use crate::provider_decision_center::AuthRequestPurpose;
use crate::provider_decision_center::ProviderDecisionCenter;
use crate::response_debug_context::extract_response_debug_context;
use crate::response_debug_context::telemetry_transport_error_message;
use crate::util::FeedbackRequestTags;
use crate::util::emit_feedback_request_tags_with_auth_env;
use http::HeaderMap;
use praxis_api::ModelsClient;
use praxis_api::RequestTelemetry;
use praxis_api::ReqwestTransport;
use praxis_api::TransportError;
use praxis_login::AuthManager;
use praxis_login::AuthMode;
use praxis_login::default_client::build_direct_reqwest_client;
use praxis_login::default_client::build_reqwest_client;
use praxis_otel::TelemetryAuthMode;
use praxis_protocol::config_types::CollaborationModeMask;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::openai_models::known_openai_compatible_picker_model_infos;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::TryLockError;
use tokio::time::timeout;
use tracing::error;
use tracing::info;
use tracing::instrument;

const MODEL_CACHE_FILE: &str = "models_cache.json";
const DEFAULT_MODEL_CACHE_TTL: Duration = Duration::from_secs(300);
const MODELS_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);
const MODELS_ENDPOINT: &str = "/models";

#[derive(Debug, Clone)]
pub struct PluginModelPreset {
    pub provider_id: String,
    pub provider: ModelProviderInfo,
    pub preset: ModelPreset,
}

pub fn plugin_model_presets_for_config(config: &Config) -> Vec<PluginModelPreset> {
    let plugins_manager = PluginsManager::new(config.praxis_home.clone());
    let plugin_outcome = plugins_manager.plugins_for_config(config);
    let runtime_catalog =
        LlmRuntimeCatalog::from_plugin_manifests(plugin_outcome.effective_llm_manifests());
    let mut presets = Vec::new();
    for (provider_id, provider) in &config.model_providers {
        presets.extend(
            runtime_catalog
                .model_infos_for_provider(provider_id, provider)
                .into_iter()
                .map(|model| PluginModelPreset {
                    provider_id: provider_id.clone(),
                    provider: provider.clone(),
                    preset: ModelPreset::from(model),
                }),
        );
    }
    presets
}

pub fn local_model_presets_for_config(config: &Config) -> Vec<PluginModelPreset> {
    crate::llm::local_models::local_model_presets_for_config(config)
        .into_iter()
        .map(|local_model| PluginModelPreset {
            provider_id: local_model.provider_id,
            provider: local_model.provider,
            preset: local_model.preset,
        })
        .collect()
}

#[derive(Clone)]
struct ModelsRequestTelemetry {
    auth_mode: Option<String>,
    auth_header_attached: bool,
    auth_header_name: Option<&'static str>,
    auth_env: AuthEnvTelemetry,
}

impl RequestTelemetry for ModelsRequestTelemetry {
    fn on_request(
        &self,
        attempt: u64,
        status: Option<http::StatusCode>,
        error: Option<&TransportError>,
        duration: Duration,
    ) {
        let success = status.is_some_and(|code| code.is_success()) && error.is_none();
        let error_message = error.map(telemetry_transport_error_message);
        let response_debug = error
            .map(extract_response_debug_context)
            .unwrap_or_default();
        let status = status.map(|status| status.as_u16());
        tracing::event!(
            target: "praxis_otel.log_only",
            tracing::Level::INFO,
            event.name = "praxis.api_request",
            duration_ms = %duration.as_millis(),
            http.response.status_code = status,
            success = success,
            error.message = error_message.as_deref(),
            attempt = attempt,
            endpoint = MODELS_ENDPOINT,
            auth.header_attached = self.auth_header_attached,
            auth.header_name = self.auth_header_name,
            auth.env_openai_api_key_present = self.auth_env.openai_api_key_env_present,
            auth.env_praxis_api_key_present = self.auth_env.praxis_api_key_env_present,
            auth.env_praxis_api_key_enabled = self.auth_env.praxis_api_key_env_enabled,
            auth.env_provider_key_name = self.auth_env.provider_env_key_name.as_deref(),
            auth.env_provider_key_present = self.auth_env.provider_env_key_present,
            auth.env_refresh_token_url_override_present = self.auth_env.refresh_token_url_override_present,
            auth.request_id = response_debug.request_id.as_deref(),
            auth.cf_ray = response_debug.cf_ray.as_deref(),
            auth.error = response_debug.auth_error.as_deref(),
            auth.error_code = response_debug.auth_error_code.as_deref(),
            auth.mode = self.auth_mode.as_deref(),
        );
        tracing::event!(
            target: "praxis_otel.trace_safe",
            tracing::Level::INFO,
            event.name = "praxis.api_request",
            duration_ms = %duration.as_millis(),
            http.response.status_code = status,
            success = success,
            error.message = error_message.as_deref(),
            attempt = attempt,
            endpoint = MODELS_ENDPOINT,
            auth.header_attached = self.auth_header_attached,
            auth.header_name = self.auth_header_name,
            auth.env_openai_api_key_present = self.auth_env.openai_api_key_env_present,
            auth.env_praxis_api_key_present = self.auth_env.praxis_api_key_env_present,
            auth.env_praxis_api_key_enabled = self.auth_env.praxis_api_key_env_enabled,
            auth.env_provider_key_name = self.auth_env.provider_env_key_name.as_deref(),
            auth.env_provider_key_present = self.auth_env.provider_env_key_present,
            auth.env_refresh_token_url_override_present = self.auth_env.refresh_token_url_override_present,
            auth.request_id = response_debug.request_id.as_deref(),
            auth.cf_ray = response_debug.cf_ray.as_deref(),
            auth.error = response_debug.auth_error.as_deref(),
            auth.error_code = response_debug.auth_error_code.as_deref(),
            auth.mode = self.auth_mode.as_deref(),
        );
        emit_feedback_request_tags_with_auth_env(
            &FeedbackRequestTags {
                endpoint: MODELS_ENDPOINT,
                auth_header_attached: self.auth_header_attached,
                auth_header_name: self.auth_header_name,
                auth_mode: self.auth_mode.as_deref(),
                auth_retry_after_unauthorized: None,
                auth_recovery_mode: None,
                auth_recovery_phase: None,
                auth_connection_reused: None,
                auth_request_id: response_debug.request_id.as_deref(),
                auth_cf_ray: response_debug.cf_ray.as_deref(),
                auth_error: response_debug.auth_error.as_deref(),
                auth_error_code: response_debug.auth_error_code.as_deref(),
                auth_recovery_followup_success: None,
                auth_recovery_followup_status: None,
            },
            &self.auth_env,
        );
    }
}

/// Strategy for refreshing available models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStrategy {
    /// Always fetch from the network, ignoring cache.
    Online,
    /// Only use cached data, never fetch from the network.
    Offline,
    /// Use cache if available and fresh, otherwise fetch from the network.
    OnlineIfUncached,
}

impl RefreshStrategy {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::OnlineIfUncached => "online_if_uncached",
        }
    }
}

impl fmt::Display for RefreshStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn merge_known_picker_models(
    remote_models: &mut Vec<ModelInfo>,
    provider_id: &str,
    provider: &ModelProviderInfo,
) {
    for model in known_openai_compatible_picker_model_infos() {
        if !crate::model_provider_info::provider_accepts_registered_model_catalog(
            provider_id,
            provider,
            model.slug.as_str(),
        ) {
            continue;
        }
        if remote_models
            .iter()
            .any(|existing| existing.slug == model.slug)
        {
            continue;
        }
        remote_models.push(model);
    }
}

/// How the manager's base catalog is sourced for the lifetime of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogMode {
    /// Start from bundled `models.json` and allow cache/network refresh updates.
    Default,
    /// Use a caller-provided catalog as authoritative and do not mutate it via refresh.
    Custom,
}

/// Coordinates remote model discovery plus cached metadata on disk.
#[derive(Debug)]
pub struct ModelsManager {
    remote_models: RwLock<Vec<ModelInfo>>,
    catalog_mode: CatalogMode,
    collaboration_modes_config: CollaborationModesConfig,
    /// Base auth manager for the session. Provider-specific auth is derived at request time.
    auth_manager: Arc<AuthManager>,
    etag: RwLock<Option<String>>,
    cache_manager: ModelsCacheManager,
    provider: ModelProviderInfo,
}

impl ModelsManager {
    /// Construct a manager scoped to the provided `AuthManager`.
    ///
    /// Uses `praxis_home` to store cached model metadata and initializes with bundled catalog
    /// When `model_catalog` is provided, it becomes the authoritative remote model list and
    /// background refreshes from `/models` are disabled.
    pub fn new(
        praxis_home: PathBuf,
        auth_manager: Arc<AuthManager>,
        model_catalog: Option<ModelsResponse>,
        collaboration_modes_config: CollaborationModesConfig,
    ) -> Self {
        Self::new_with_provider(
            praxis_home,
            auth_manager,
            model_catalog,
            collaboration_modes_config,
            ModelProviderInfo::create_openai_provider(/*base_url*/ None),
        )
    }

    /// Construct a manager with an explicit provider used for remote model refreshes.
    pub fn new_with_provider(
        praxis_home: PathBuf,
        auth_manager: Arc<AuthManager>,
        model_catalog: Option<ModelsResponse>,
        collaboration_modes_config: CollaborationModesConfig,
        provider: ModelProviderInfo,
    ) -> Self {
        let cache_path = praxis_home.join(MODEL_CACHE_FILE);
        let cache_manager = ModelsCacheManager::new(cache_path, DEFAULT_MODEL_CACHE_TTL);
        let catalog_mode = if model_catalog.is_some() {
            CatalogMode::Custom
        } else {
            CatalogMode::Default
        };
        let remote_models = model_catalog
            .map(|catalog| catalog.models)
            .unwrap_or_else(|| {
                if provider.is_openai() {
                    Self::load_remote_models_from_file()
                        .unwrap_or_else(|err| panic!("failed to load bundled models.json: {err}"))
                } else {
                    Vec::new()
                }
            });
        Self {
            remote_models: RwLock::new(remote_models),
            catalog_mode,
            collaboration_modes_config,
            auth_manager,
            etag: RwLock::new(None),
            cache_manager,
            provider,
        }
    }

    /// List all available models, refreshing according to the specified strategy.
    ///
    /// Returns model presets sorted by priority and filtered by auth mode and visibility.
    #[instrument(
        level = "info",
        skip(self),
        fields(refresh_strategy = %refresh_strategy)
    )]
    pub async fn list_models(&self, refresh_strategy: RefreshStrategy) -> Vec<ModelPreset> {
        if let Err(err) = self.refresh_available_models(refresh_strategy).await {
            error!("failed to refresh available models: {err}");
        }
        let remote_models = self.get_remote_models().await;
        self.build_available_models(remote_models)
    }

    /// List models using the provider selected by the current config.
    ///
    /// The manager's primary provider still owns network refresh for now. For a different provider,
    /// return the best local catalog snapshot instead of leaking the primary provider's catalog into
    /// the current turn.
    pub async fn list_models_for_config(
        &self,
        config: &Config,
        refresh_strategy: RefreshStrategy,
    ) -> Vec<ModelPreset> {
        if crate::llm::local_models::config_uses_native_local_provider(config) {
            return local_model_presets_for_config(config)
                .into_iter()
                .map(|local_model| local_model.preset)
                .collect();
        }

        if self.provider == config.model_provider {
            return self.list_models(refresh_strategy).await;
        }

        let remote_models = if matches!(self.catalog_mode, CatalogMode::Custom) {
            self.get_remote_models().await
        } else {
            Self::static_remote_models_for_provider(&config.model_provider)
        };
        self.build_available_models_for_provider(
            remote_models,
            config.model_provider_id.as_str(),
            &config.model_provider,
        )
    }

    /// List collaboration mode presets.
    ///
    /// Returns a static set of presets seeded with the configured model.
    pub fn list_collaboration_modes(&self) -> Vec<CollaborationModeMask> {
        self.list_collaboration_modes_for_config(self.collaboration_modes_config)
    }

    pub fn list_collaboration_modes_for_config(
        &self,
        collaboration_modes_config: CollaborationModesConfig,
    ) -> Vec<CollaborationModeMask> {
        builtin_collaboration_mode_presets(collaboration_modes_config)
    }

    /// Attempt to list models without blocking, using the current cached state.
    ///
    /// Returns an error if the internal lock cannot be acquired.
    pub fn try_list_models(&self) -> Result<Vec<ModelPreset>, TryLockError> {
        let remote_models = self.try_get_remote_models()?;
        Ok(self.build_available_models(remote_models))
    }

    pub fn try_list_models_for_config(
        &self,
        config: &Config,
    ) -> Result<Vec<ModelPreset>, TryLockError> {
        if crate::llm::local_models::config_uses_native_local_provider(config) {
            return Ok(local_model_presets_for_config(config)
                .into_iter()
                .map(|local_model| local_model.preset)
                .collect());
        }

        if self.provider == config.model_provider {
            return self.try_list_models();
        }

        let remote_models = if matches!(self.catalog_mode, CatalogMode::Custom) {
            self.try_get_remote_models()?
        } else {
            Self::static_remote_models_for_provider(&config.model_provider)
        };
        Ok(self.build_available_models_for_provider(
            remote_models,
            config.model_provider_id.as_str(),
            &config.model_provider,
        ))
    }

    // todo(aibrahim): should be visible to core only and sent on session_configured event
    /// Get the model identifier to use, refreshing according to the specified strategy.
    ///
    /// If `model` is provided, returns it directly. Otherwise selects the default based on
    /// auth mode and available models.
    #[instrument(
        level = "info",
        skip(self, model),
        fields(
            model.provided = model.is_some(),
            refresh_strategy = %refresh_strategy
        )
    )]
    pub async fn get_default_model(
        &self,
        model: &Option<String>,
        refresh_strategy: RefreshStrategy,
    ) -> String {
        if let Some(model) = model.as_ref() {
            return model.to_string();
        }
        if let Err(err) = self.refresh_available_models(refresh_strategy).await {
            error!("failed to refresh available models: {err}");
        }
        let remote_models = self.get_remote_models().await;
        let available = self.build_available_models(remote_models);
        available
            .iter()
            .find(|model| model.is_default)
            .or_else(|| available.first())
            .map(|model| model.model.clone())
            .unwrap_or_default()
    }

    pub async fn get_default_model_for_config(
        &self,
        model: &Option<String>,
        refresh_strategy: RefreshStrategy,
        config: &Config,
    ) -> String {
        if let Some(model) = model.as_ref() {
            return model.to_string();
        }

        let available = self.list_models_for_config(config, refresh_strategy).await;
        available
            .iter()
            .find(|model| model.is_default)
            .or_else(|| available.first())
            .map(|model| model.model.clone())
            .unwrap_or_default()
    }

    // todo(aibrahim): look if we can tighten it to pub(crate)
    /// Look up model metadata, applying remote overrides and config adjustments.
    #[instrument(level = "info", skip(self, config), fields(model = model))]
    pub async fn get_model_info(&self, model: &str, config: &Config) -> ModelInfo {
        let remote_models = self.remote_models_for_config(config).await;
        Self::construct_model_info_from_candidates(model, &remote_models, config)
    }

    fn find_model_by_longest_prefix(model: &str, candidates: &[ModelInfo]) -> Option<ModelInfo> {
        let mut best: Option<ModelInfo> = None;
        for candidate in candidates {
            if !model.starts_with(&candidate.slug) {
                continue;
            }
            let is_better_match = if let Some(current) = best.as_ref() {
                candidate.slug.len() > current.slug.len()
            } else {
                true
            };
            if is_better_match {
                best = Some(candidate.clone());
            }
        }
        best
    }

    /// Retry metadata lookup for a single namespaced slug like `namespace/model-name`.
    ///
    /// This only strips one leading namespace segment and only when the namespace is ASCII
    /// alphanumeric/underscore (`\\w+`) to avoid broadly matching arbitrary aliases.
    fn find_model_by_namespaced_suffix(model: &str, candidates: &[ModelInfo]) -> Option<ModelInfo> {
        let (namespace, suffix) = model.split_once('/')?;
        if suffix.contains('/') {
            return None;
        }
        if !namespace
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        Self::find_model_by_longest_prefix(suffix, candidates)
    }

    fn construct_model_info_from_candidates(
        model: &str,
        candidates: &[ModelInfo],
        config: &Config,
    ) -> ModelInfo {
        if crate::llm::local_models::config_uses_native_local_provider(config)
            && let Some(model_info) =
                crate::llm::local_models::local_model_info_for_config(config, model)
        {
            return model_info::with_config_overrides(model_info, config);
        }

        // First use the normal longest-prefix match. If that misses, allow a narrowly scoped
        // retry for namespaced slugs like `custom/gpt-5.3-codex`.
        let remote = Self::find_model_by_longest_prefix(model, candidates)
            .or_else(|| Self::find_model_by_namespaced_suffix(model, candidates));
        let model_info = if let Some(remote) = remote {
            ModelInfo {
                slug: model.to_string(),
                used_fallback_model_metadata: false,
                ..remote
            }
        } else {
            model_info::model_info_from_slug(model)
        };
        let model_info = model_info::with_known_model_capability_overrides(model_info);
        model_info::with_config_overrides(model_info, config)
    }

    /// Refresh models if the provided ETag differs from the cached ETag.
    ///
    /// Uses `Online` strategy to fetch latest models when ETags differ.
    pub(crate) async fn refresh_if_new_etag(&self, etag: String) {
        let current_etag = self.get_etag().await;
        if current_etag.clone().is_some() && current_etag.as_deref() == Some(etag.as_str()) {
            if let Err(err) = self.cache_manager.renew_cache_ttl().await {
                error!("failed to renew cache TTL: {err}");
            }
            return;
        }
        if let Err(err) = self.refresh_available_models(RefreshStrategy::Online).await {
            error!("failed to refresh available models: {err}");
        }
    }

    /// Refresh available models according to the specified strategy.
    async fn refresh_available_models(&self, refresh_strategy: RefreshStrategy) -> CoreResult<()> {
        // don't override the custom model catalog if one was provided by the user
        if matches!(self.catalog_mode, CatalogMode::Custom) {
            return Ok(());
        }

        if !self.provider.is_openai() && !self.provider.has_command_auth() {
            if matches!(
                refresh_strategy,
                RefreshStrategy::Offline | RefreshStrategy::OnlineIfUncached
            ) {
                self.try_load_cache().await;
                return Ok(());
            }
        }

        let auth_manager = self.auth_manager_for_provider(&self.provider);
        if auth_manager.auth_mode() != Some(AuthMode::Chatgpt) && !self.provider.has_command_auth()
        {
            if matches!(
                refresh_strategy,
                RefreshStrategy::Offline | RefreshStrategy::OnlineIfUncached
            ) {
                self.try_load_cache().await;
            }
            return Ok(());
        }

        match refresh_strategy {
            RefreshStrategy::Offline => {
                // Only try to load from cache, never fetch
                self.try_load_cache().await;
                Ok(())
            }
            RefreshStrategy::OnlineIfUncached => {
                // Try cache first, fall back to online if unavailable
                if self.try_load_cache().await {
                    info!("models cache: using cached models for OnlineIfUncached");
                    return Ok(());
                }
                info!("models cache: cache miss, fetching remote models");
                self.fetch_and_update_models().await
            }
            RefreshStrategy::Online => {
                // Always fetch from network
                self.fetch_and_update_models().await
            }
        }
    }

    async fn fetch_and_update_models(&self) -> CoreResult<()> {
        let _timer =
            praxis_otel::start_global_timer("praxis.remote_models.fetch_update.duration_ms", &[]);
        let auth_manager = self.auth_manager_for_provider(&self.provider);
        let setup = ProviderDecisionCenter::new(Some(Arc::clone(&auth_manager)))
            .setup_provider(&self.provider, AuthRequestPurpose::ModelList)
            .await?;
        let auth_env = setup.auth_env_telemetry.clone();
        let transport = ReqwestTransport::new(self.build_models_reqwest_client());
        let request_telemetry: Arc<dyn RequestTelemetry> = Arc::new(ModelsRequestTelemetry {
            auth_mode: setup
                .auth_mode
                .map(|mode| TelemetryAuthMode::from(mode).to_string()),
            auth_header_attached: setup.api_auth.auth_header_attached(),
            auth_header_name: setup.api_auth.auth_header_name(),
            auth_env,
        });
        let client = ModelsClient::new(transport, setup.api_provider, setup.api_auth)
            .with_telemetry(Some(request_telemetry));

        let client_version = crate::models_manager::client_version_to_whole();
        let (models, etag) = timeout(
            MODELS_REFRESH_TIMEOUT,
            client.list_models(&client_version, HeaderMap::new()),
        )
        .await
        .map_err(|_| PraxisErr::Timeout)?
        .map_err(map_api_error)?;

        self.apply_remote_models(models.clone()).await;
        *self.etag.write().await = etag.clone();
        self.cache_manager
            .persist_cache(&models, etag, client_version)
            .await;
        Ok(())
    }

    async fn get_etag(&self) -> Option<String> {
        self.etag.read().await.clone()
    }

    fn build_models_reqwest_client(&self) -> reqwest::Client {
        if self.provider.is_openai() || self.provider.has_command_auth() {
            build_reqwest_client()
        } else {
            build_direct_reqwest_client()
        }
    }

    /// Replace the cached remote models and rebuild the derived presets list.
    async fn apply_remote_models(&self, models: Vec<ModelInfo>) {
        let mut existing_models = Self::load_remote_models_from_file().unwrap_or_default();
        for model in models {
            if let Some(existing_index) = existing_models
                .iter()
                .position(|existing| existing.slug == model.slug)
            {
                existing_models[existing_index] = model;
            } else {
                existing_models.push(model);
            }
        }
        *self.remote_models.write().await = existing_models;
    }

    fn load_remote_models_from_file() -> Result<Vec<ModelInfo>, std::io::Error> {
        let file_contents = include_str!("../../models.json");
        let response: ModelsResponse = serde_json::from_str(file_contents)?;
        Ok(response.models)
    }

    fn static_remote_models_for_provider(provider: &ModelProviderInfo) -> Vec<ModelInfo> {
        if provider.is_openai() {
            Self::load_remote_models_from_file().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    async fn remote_models_for_config(&self, config: &Config) -> Vec<ModelInfo> {
        if self.provider == config.model_provider {
            self.get_remote_models().await
        } else if matches!(self.catalog_mode, CatalogMode::Custom) {
            self.get_remote_models().await
        } else {
            Self::static_remote_models_for_provider(&config.model_provider)
        }
    }

    /// Attempt to satisfy the refresh from the cache when it matches the provider and TTL.
    async fn try_load_cache(&self) -> bool {
        let _timer =
            praxis_otel::start_global_timer("praxis.remote_models.load_cache.duration_ms", &[]);
        let client_version = crate::models_manager::client_version_to_whole();
        info!(client_version, "models cache: evaluating cache eligibility");
        let cache = match self.cache_manager.load_fresh(&client_version).await {
            Some(cache) => cache,
            None => {
                info!("models cache: no usable cache entry");
                return false;
            }
        };
        let models = cache.models.clone();
        *self.etag.write().await = cache.etag.clone();
        self.apply_remote_models(models.clone()).await;
        info!(
            models_count = models.len(),
            etag = ?cache.etag,
            "models cache: cache entry applied"
        );
        true
    }

    /// Build picker-ready presets from the active catalog snapshot.
    fn build_available_models(&self, remote_models: Vec<ModelInfo>) -> Vec<ModelPreset> {
        self.build_available_models_for_provider(
            remote_models,
            model_catalog_provider_id_for_provider(&self.provider),
            &self.provider,
        )
    }

    fn build_available_models_for_provider(
        &self,
        mut remote_models: Vec<ModelInfo>,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> Vec<ModelPreset> {
        if !matches!(self.catalog_mode, CatalogMode::Custom) {
            merge_known_picker_models(&mut remote_models, provider_id, provider);
        }
        remote_models.sort_by(|a, b| a.priority.cmp(&b.priority));

        let mut presets: Vec<ModelPreset> = remote_models
            .into_iter()
            .filter(|model| {
                if matches!(self.catalog_mode, CatalogMode::Custom) {
                    return true;
                }
                crate::model_provider_info::provider_accepts_registered_model_catalog(
                    provider_id,
                    provider,
                    model.slug.as_str(),
                )
            })
            .map(Into::into)
            .collect();
        let auth_manager = self.auth_manager_for_provider(provider);
        let chatgpt_mode = matches!(auth_manager.auth_mode(), Some(AuthMode::Chatgpt));
        presets = ModelPreset::filter_by_auth(presets, chatgpt_mode);

        ModelPreset::mark_default_by_picker_visibility(&mut presets);

        presets
    }

    fn auth_manager_for_provider(&self, provider: &ModelProviderInfo) -> Arc<AuthManager> {
        ProviderDecisionCenter::provider_auth_manager(
            Some(Arc::clone(&self.auth_manager)),
            provider,
        )
        .unwrap_or_else(|| Arc::clone(&self.auth_manager))
    }

    async fn get_remote_models(&self) -> Vec<ModelInfo> {
        self.remote_models.read().await.clone()
    }

    fn try_get_remote_models(&self) -> Result<Vec<ModelInfo>, TryLockError> {
        Ok(self.remote_models.try_read()?.clone())
    }

    /// Construct a manager with a specific provider for testing.
    pub(crate) fn with_provider_for_tests(
        praxis_home: PathBuf,
        auth_manager: Arc<AuthManager>,
        provider: ModelProviderInfo,
    ) -> Self {
        Self::new_with_provider(
            praxis_home,
            auth_manager,
            /*model_catalog*/ None,
            CollaborationModesConfig::default(),
            provider,
        )
    }

    /// Get model identifier without consulting remote state or cache.
    pub(crate) fn get_model_offline_for_tests(model: Option<&str>) -> String {
        if let Some(model) = model {
            return model.to_string();
        }
        let mut models = Self::load_remote_models_from_file().unwrap_or_default();
        models.sort_by(|a, b| a.priority.cmp(&b.priority));
        let presets: Vec<ModelPreset> = models.into_iter().map(Into::into).collect();
        presets
            .iter()
            .find(|preset| preset.show_in_picker)
            .or_else(|| presets.first())
            .map(|preset| preset.model.clone())
            .unwrap_or_default()
    }

    /// Build `ModelInfo` without consulting remote state or cache.
    pub(crate) fn construct_model_info_offline_for_tests(
        model: &str,
        config: &Config,
    ) -> ModelInfo {
        let candidates: &[ModelInfo] = if let Some(model_catalog) = config.model_catalog.as_ref() {
            &model_catalog.models
        } else {
            &[]
        };
        Self::construct_model_info_from_candidates(model, candidates, config)
    }
}

fn model_catalog_provider_id_for_provider(provider: &ModelProviderInfo) -> &str {
    if provider.is_openai() {
        OPENAI_PROVIDER_ID
    } else {
        provider.name.as_str()
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

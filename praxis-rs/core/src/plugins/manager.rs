use super::LoadedPlugin;
use super::PluginLoadOutcome;
use super::PluginManifestPaths;
use super::curated_plugins_repo_path;
use super::load_plugin_manifest;
use super::manifest::PluginManifestInterface;
use super::manifest::PluginManifestLlm;
use super::marketplace::MarketplaceError;
use super::marketplace::MarketplaceInterface;
use super::marketplace::MarketplaceListError;
use super::marketplace::MarketplacePluginAuthPolicy;
use super::marketplace::MarketplacePluginPolicy;
use super::marketplace::MarketplacePluginSource;
use super::marketplace::ResolvedMarketplacePlugin;
use super::marketplace::list_marketplaces;
use super::marketplace::load_marketplace;
use super::marketplace::resolve_marketplace_plugin;
use super::read_curated_plugins_sha;
use super::remote::RemotePluginFetchError;
use super::remote::RemotePluginMutationError;
use super::remote::enable_remote_plugin;
use super::remote::fetch_remote_featured_plugin_ids;
use super::remote::fetch_remote_plugin_status;
use super::remote::uninstall_remote_plugin;
use super::startup_sync::start_startup_remote_plugin_sync_once;
use super::store::PluginInstallResult as StorePluginInstallResult;
use super::store::PluginStore;
use super::store::PluginStoreError;
use super::sync_openai_plugins_repo;
use crate::SkillMetadata;
use crate::config::CONFIG_TOML_FILE;
use crate::config::Config;
use crate::config::ConfigService;
use crate::config::ConfigServiceError;
use crate::config::ConfigValueWriteParams;
use crate::config::MergeStrategy;
use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::config_loader::ConfigLayerStack;
use crate::config_rules::SkillConfigRules;
use crate::config_rules::resolve_disabled_skill_paths;
use crate::config_rules::skill_config_rules_from_stack;
use crate::loader::SkillRoot;
use crate::loader::load_skills_from_roots;
use praxis_analytics::AnalyticsEventsClient;
use praxis_config::types::McpServerConfig;
use praxis_config::types::PluginConfig;
use praxis_config::types::PluginMarketplaceProviderConfig;
use praxis_features::Feature;
use praxis_login::AuthManager;
use praxis_login::CodexAuth;
use praxis_plugin::AppConnectorId;
use praxis_plugin::PluginActivationDelta;
use praxis_plugin::PluginCapabilityChanges;
use praxis_plugin::PluginCapabilitySummary;
use praxis_plugin::PluginId;
use praxis_plugin::PluginIdError;
use praxis_plugin::PluginMarketplaceProviderSource;
use praxis_plugin::PluginMarketplaceRef;
use praxis_plugin::PluginMarketplaceSyncOutcome;
use praxis_plugin::PluginTelemetryMetadata;
use praxis_plugin::prompt_safe_plugin_description;
use praxis_protocol::protocol::Product;
use praxis_protocol::protocol::SkillScope;
use praxis_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use tempfile::TempDir;
use tokio::sync::Mutex;
use toml_edit::value;
use tracing::info;
use tracing::warn;

const DEFAULT_SKILLS_DIR_NAME: &str = "skills";
const DEFAULT_MCP_CONFIG_FILE: &str = ".mcp.json";
const DEFAULT_APP_CONFIG_FILE: &str = ".app.json";
const MARKETPLACE_PROVIDER_CACHE_DIR: &str = "plugins/marketplaces";
const MARKETPLACE_PROVIDER_GIT_TIMEOUT: Duration = Duration::from_secs(45);
const MARKETPLACE_PROVIDER_STALE_TEMP_DIR_MAX_AGE: Duration = Duration::from_secs(10 * 60);
pub const OPENAI_CURATED_MARKETPLACE_NAME: &str = "openai-curated";
pub const OPENAI_CURATED_MARKETPLACE_DISPLAY_NAME: &str = "OpenAI Curated";
static CURATED_REPO_SYNC_STARTED: AtomicBool = AtomicBool::new(false);
const FEATURED_PLUGIN_IDS_CACHE_TTL: std::time::Duration =
    std::time::Duration::from_secs(60 * 60 * 3);

#[derive(Clone, PartialEq, Eq)]
struct FeaturedPluginIdsCacheKey {
    chatgpt_base_url: String,
    account_id: Option<String>,
    chatgpt_user_id: Option<String>,
    is_workspace_account: bool,
}

#[derive(Clone)]
struct CachedFeaturedPluginIds {
    key: FeaturedPluginIdsCacheKey,
    expires_at: Instant,
    featured_plugin_ids: Vec<String>,
}

fn featured_plugin_ids_cache_key(
    config: &Config,
    auth: Option<&CodexAuth>,
) -> FeaturedPluginIdsCacheKey {
    let token_data = auth.and_then(|auth| auth.get_token_data().ok());
    let account_id = token_data
        .as_ref()
        .and_then(|token_data| token_data.account_id.clone());
    let chatgpt_user_id = token_data
        .as_ref()
        .and_then(|token_data| token_data.id_token.chatgpt_user_id.clone());
    let is_workspace_account = token_data
        .as_ref()
        .is_some_and(|token_data| token_data.id_token.is_workspace_account());
    FeaturedPluginIdsCacheKey {
        chatgpt_base_url: config.chatgpt_base_url.clone(),
        account_id,
        chatgpt_user_id,
        is_workspace_account,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstallRequest {
    pub plugin_name: String,
    pub marketplace_path: AbsolutePathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginReadRequest {
    pub plugin_name: String,
    pub marketplace_path: AbsolutePathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginInstallOutcome {
    pub plugin_id: PluginId,
    pub plugin_version: String,
    pub installed_path: AbsolutePathBuf,
    pub auth_policy: MarketplacePluginAuthPolicy,
    pub activation_delta: PluginActivationDelta<McpServerConfig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginReadOutcome {
    pub marketplace_name: String,
    pub marketplace_path: AbsolutePathBuf,
    pub plugin: PluginDetail,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source: MarketplacePluginSource,
    pub policy: MarketplacePluginPolicy,
    pub interface: Option<PluginManifestInterface>,
    pub llm: Option<PluginManifestLlm>,
    pub installed: bool,
    pub enabled: bool,
    pub skills: Vec<SkillMetadata>,
    pub disabled_skill_paths: HashSet<PathBuf>,
    pub apps: Vec<AppConnectorId>,
    pub mcp_server_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredMarketplace {
    pub name: String,
    pub path: AbsolutePathBuf,
    pub interface: Option<MarketplaceInterface>,
    pub plugins: Vec<ConfiguredMarketplacePlugin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredMarketplacePlugin {
    pub id: String,
    pub name: String,
    pub source: MarketplacePluginSource,
    pub policy: MarketplacePluginPolicy,
    pub interface: Option<PluginManifestInterface>,
    pub llm: Option<PluginManifestLlm>,
    pub installed: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfiguredMarketplaceListOutcome {
    pub marketplaces: Vec<ConfiguredMarketplace>,
    pub errors: Vec<MarketplaceListError>,
}

impl From<PluginDetail> for PluginCapabilitySummary {
    fn from(value: PluginDetail) -> Self {
        let has_skills = value.skills.iter().any(|skill| {
            !value
                .disabled_skill_paths
                .contains(&skill.path_to_skills_md)
        });
        Self {
            config_name: value.id,
            display_name: value.name,
            description: prompt_safe_plugin_description(value.description.as_deref()),
            has_skills,
            has_llm: value.llm.is_some(),
            mcp_server_names: value.mcp_server_names,
            app_connector_ids: value.apps,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemotePluginSyncResult {
    /// Plugin ids newly installed into the local plugin cache.
    pub installed_plugin_ids: Vec<String>,
    /// Plugin ids whose local config was changed to enabled.
    pub enabled_plugin_ids: Vec<String>,
    /// Plugin ids whose local config was changed to disabled.
    /// This is not populated by `sync_plugins_from_remote`.
    pub disabled_plugin_ids: Vec<String>,
    /// Plugin ids removed from local cache or plugin config.
    pub uninstalled_plugin_ids: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginRemoteSyncError {
    #[error("chatgpt authentication required to sync remote plugins")]
    AuthRequired,

    #[error(
        "chatgpt authentication required to sync remote plugins; api key auth is not supported"
    )]
    UnsupportedAuthMode,

    #[error("failed to read auth token for remote plugin sync: {0}")]
    AuthToken(#[source] std::io::Error),

    #[error("failed to send remote plugin sync request to {url}: {source}")]
    Request {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("remote plugin sync request to {url} failed with status {status}: {body}")]
    UnexpectedStatus {
        url: String,
        status: reqwest::StatusCode,
        body: String,
    },

    #[error("failed to parse remote plugin sync response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("local curated marketplace is not available")]
    LocalMarketplaceNotFound,

    #[error("remote marketplace `{marketplace_name}` is not available locally")]
    UnknownRemoteMarketplace { marketplace_name: String },

    #[error("duplicate remote plugin `{plugin_name}` in sync response")]
    DuplicateRemotePlugin { plugin_name: String },

    #[error(
        "remote plugin `{plugin_name}` was not found in local marketplace `{marketplace_name}`"
    )]
    UnknownRemotePlugin {
        plugin_name: String,
        marketplace_name: String,
    },

    #[error("{0}")]
    InvalidPluginId(#[from] PluginIdError),

    #[error("{0}")]
    Marketplace(#[from] MarketplaceError),

    #[error("{0}")]
    Store(#[from] PluginStoreError),

    #[error("{0}")]
    Config(#[from] anyhow::Error),

    #[error("failed to join remote plugin sync task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl PluginRemoteSyncError {
    fn join(source: tokio::task::JoinError) -> Self {
        Self::Join(source)
    }
}

impl From<RemotePluginFetchError> for PluginRemoteSyncError {
    fn from(value: RemotePluginFetchError) -> Self {
        match value {
            RemotePluginFetchError::AuthRequired => Self::AuthRequired,
            RemotePluginFetchError::UnsupportedAuthMode => Self::UnsupportedAuthMode,
            RemotePluginFetchError::AuthToken(source) => Self::AuthToken(source),
            RemotePluginFetchError::Request { url, source } => Self::Request { url, source },
            RemotePluginFetchError::UnexpectedStatus { url, status, body } => {
                Self::UnexpectedStatus { url, status, body }
            }
            RemotePluginFetchError::Decode { url, source } => Self::Decode { url, source },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginMarketplaceProviderSyncError {
    #[error("plugin marketplace `{0}` is not configured")]
    NotConfigured(String),

    #[error("plugin marketplace `{0}` is disabled")]
    Disabled(String),

    #[error("plugin marketplace `{marketplace_name}` has unsupported provider `{provider}`")]
    UnsupportedProvider {
        marketplace_name: String,
        provider: &'static str,
    },

    #[error("invalid plugin marketplace name `{0}`")]
    InvalidMarketplaceName(String),

    #[error("failed to sync git plugin marketplace `{marketplace_name}`: {message}")]
    Git {
        marketplace_name: String,
        message: String,
    },

    #[error("failed to join marketplace provider sync task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub struct PluginsManager {
    praxis_home: PathBuf,
    store: PluginStore,
    featured_plugin_ids_cache: RwLock<Option<CachedFeaturedPluginIds>>,
    cached_enabled_outcome: RwLock<Option<PluginLoadOutcome>>,
    remote_sync_lock: Mutex<()>,
    restriction_product: Option<Product>,
    analytics_events_client: RwLock<Option<AnalyticsEventsClient>>,
}

impl PluginsManager {
    pub fn new(praxis_home: PathBuf) -> Self {
        Self::new_with_restriction_product(praxis_home, Some(Product::Praxis))
    }

    pub fn new_with_restriction_product(
        praxis_home: PathBuf,
        restriction_product: Option<Product>,
    ) -> Self {
        // Product restrictions are enforced at marketplace admission time for a given PRAXIS_HOME:
        // listing, install, and curated refresh all consult this restriction context before new
        // plugins enter local config or cache. After admission, runtime plugin loading trusts the
        // contents of that PRAXIS_HOME and does not re-filter configured plugins by product, so
        // already-admitted plugins may continue exposing MCP servers/tools from shared local state.
        //
        // This assumes a single PRAXIS_HOME is only used by one product.
        Self {
            praxis_home: praxis_home.clone(),
            store: PluginStore::new(praxis_home),
            featured_plugin_ids_cache: RwLock::new(None),
            cached_enabled_outcome: RwLock::new(None),
            remote_sync_lock: Mutex::new(()),
            restriction_product,
            analytics_events_client: RwLock::new(None),
        }
    }

    pub fn set_analytics_events_client(&self, analytics_events_client: AnalyticsEventsClient) {
        let mut stored_client = match self.analytics_events_client.write() {
            Ok(client_guard) => client_guard,
            Err(err) => err.into_inner(),
        };
        *stored_client = Some(analytics_events_client);
    }

    fn restriction_product_matches(&self, products: Option<&[Product]>) -> bool {
        match products {
            None => true,
            Some([]) => false,
            Some(products) => self
                .restriction_product
                .is_some_and(|product| product.matches_product_restriction(products)),
        }
    }

    pub fn plugins_for_config(&self, config: &Config) -> PluginLoadOutcome {
        self.plugins_for_config_with_force_reload(config, /*force_reload*/ false)
    }

    pub(crate) fn plugins_for_config_with_force_reload(
        &self,
        config: &Config,
        force_reload: bool,
    ) -> PluginLoadOutcome {
        if !config.features.enabled(Feature::Plugins) {
            return PluginLoadOutcome::default();
        }

        if !force_reload && let Some(outcome) = self.cached_enabled_outcome() {
            return outcome;
        }

        let outcome = load_plugins_from_layer_stack(
            &config.config_layer_stack,
            &self.store,
            self.restriction_product,
        );
        log_plugin_load_errors(&outcome);
        let mut cache = match self.cached_enabled_outcome.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *cache = Some(outcome.clone());
        outcome
    }

    pub fn clear_cache(&self) {
        let mut cached_enabled_outcome = match self.cached_enabled_outcome.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        let mut featured_plugin_ids_cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *featured_plugin_ids_cache = None;
        *cached_enabled_outcome = None;
    }

    /// Resolve plugin skill roots for a config layer stack without touching the plugins cache.
    pub fn effective_skill_roots_for_layer_stack(
        &self,
        config_layer_stack: &ConfigLayerStack,
        plugins_feature_enabled: bool,
    ) -> Vec<PathBuf> {
        if !plugins_feature_enabled {
            return Vec::new();
        }
        load_plugins_from_layer_stack(config_layer_stack, &self.store, self.restriction_product)
            .effective_skill_roots()
    }

    fn cached_enabled_outcome(&self) -> Option<PluginLoadOutcome> {
        match self.cached_enabled_outcome.read() {
            Ok(cache) => cache.clone(),
            Err(err) => err.into_inner().clone(),
        }
    }

    fn cached_featured_plugin_ids(
        &self,
        cache_key: &FeaturedPluginIdsCacheKey,
    ) -> Option<Vec<String>> {
        {
            let cache = match self.featured_plugin_ids_cache.read() {
                Ok(cache) => cache,
                Err(err) => err.into_inner(),
            };
            let now = Instant::now();
            if let Some(cached) = cache.as_ref()
                && now < cached.expires_at
                && cached.key == *cache_key
            {
                return Some(cached.featured_plugin_ids.clone());
            }
        }

        let mut cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        let now = Instant::now();
        if cache
            .as_ref()
            .is_some_and(|cached| now >= cached.expires_at || cached.key != *cache_key)
        {
            *cache = None;
        }
        None
    }

    fn write_featured_plugin_ids_cache(
        &self,
        cache_key: FeaturedPluginIdsCacheKey,
        featured_plugin_ids: &[String],
    ) {
        let mut cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *cache = Some(CachedFeaturedPluginIds {
            key: cache_key,
            expires_at: Instant::now() + FEATURED_PLUGIN_IDS_CACHE_TTL,
            featured_plugin_ids: featured_plugin_ids.to_vec(),
        });
    }

    pub async fn featured_plugin_ids_for_config(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
    ) -> Result<Vec<String>, RemotePluginFetchError> {
        if !config.features.enabled(Feature::Plugins) {
            return Ok(Vec::new());
        }

        let cache_key = featured_plugin_ids_cache_key(config, auth);
        if let Some(featured_plugin_ids) = self.cached_featured_plugin_ids(&cache_key) {
            return Ok(featured_plugin_ids);
        }
        let featured_plugin_ids =
            fetch_remote_featured_plugin_ids(config, auth, self.restriction_product).await?;
        self.write_featured_plugin_ids_cache(cache_key, &featured_plugin_ids);
        Ok(featured_plugin_ids)
    }

    pub async fn install_plugin(
        &self,
        request: PluginInstallRequest,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let resolved = resolve_marketplace_plugin(
            &request.marketplace_path,
            &request.plugin_name,
            self.restriction_product,
        )?;
        self.install_resolved_plugin(resolved).await
    }

    pub async fn install_plugin_with_remote_sync(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
        request: PluginInstallRequest,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let resolved = resolve_marketplace_plugin(
            &request.marketplace_path,
            &request.plugin_name,
            self.restriction_product,
        )?;
        let plugin_id = resolved.plugin_id.as_key();
        // This only forwards the backend mutation before the local install flow. We rely on
        // `plugin/catalog/list(forceRemoteSync=true)` to sync local state rather than doing an extra
        // reconcile pass here.
        enable_remote_plugin(config, auth, &plugin_id)
            .await
            .map_err(PluginInstallError::from)?;
        self.install_resolved_plugin(resolved).await
    }

    async fn install_resolved_plugin(
        &self,
        resolved: ResolvedMarketplacePlugin,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let auth_policy = resolved.auth_policy;
        let plugin_version =
            if resolved.plugin_id.marketplace_name == OPENAI_CURATED_MARKETPLACE_NAME {
                Some(
                    read_curated_plugins_sha(self.praxis_home.as_path()).ok_or_else(|| {
                        PluginStoreError::Invalid(
                            "local curated marketplace sha is not available".to_string(),
                        )
                    })?,
                )
            } else {
                None
            };
        let store = self.store.clone();
        let result: StorePluginInstallResult = tokio::task::spawn_blocking(move || {
            if let Some(plugin_version) = plugin_version {
                store.install_with_version(resolved.source_path, resolved.plugin_id, plugin_version)
            } else {
                store.install(resolved.source_path, resolved.plugin_id)
            }
        })
        .await
        .map_err(PluginInstallError::join)??;

        ConfigService::new_with_defaults(self.praxis_home.clone())
            .write_value(ConfigValueWriteParams {
                key_path: format!("plugins.{}", result.plugin_id.as_key()),
                value: json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Replace,
                file_path: None,
                expected_version: None,
            })
            .await
            .map(|_| ())
            .map_err(PluginInstallError::from)?;

        let analytics_events_client = match self.analytics_events_client.read() {
            Ok(client) => client.clone(),
            Err(err) => err.into_inner().clone(),
        };
        if let Some(analytics_events_client) = analytics_events_client {
            analytics_events_client.track_plugin_installed(plugin_telemetry_metadata_from_root(
                &result.plugin_id,
                result.installed_path.as_path(),
            ));
        }

        let activation_delta =
            plugin_activation_delta_from_root(&result.plugin_id, result.installed_path.as_path());

        Ok(PluginInstallOutcome {
            plugin_id: result.plugin_id,
            plugin_version: result.plugin_version,
            installed_path: result.installed_path,
            auth_policy,
            activation_delta,
        })
    }

    pub async fn uninstall_plugin(
        &self,
        plugin_id: String,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginUninstallError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        self.uninstall_plugin_id(plugin_id).await
    }

    pub async fn uninstall_plugin_with_remote_sync(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
        plugin_id: String,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginUninstallError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        let plugin_key = plugin_id.as_key();
        // This only forwards the backend mutation before the local uninstall flow. We rely on
        // `plugin/catalog/list(forceRemoteSync=true)` to sync local state rather than doing an extra
        // reconcile pass here.
        uninstall_remote_plugin(config, auth, &plugin_key)
            .await
            .map_err(PluginUninstallError::from)?;
        self.uninstall_plugin_id(plugin_id).await
    }

    pub async fn set_plugin_enabled(
        &self,
        plugin_id: String,
        enabled: bool,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginSetEnabledError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        let plugin_root = self
            .store
            .active_plugin_root(&plugin_id)
            .ok_or_else(|| PluginSetEnabledError::NotInstalled(plugin_id.as_key()))?;
        let activation_delta = plugin_activation_delta_from_root(&plugin_id, plugin_root.as_path());

        ConfigService::new_with_defaults(self.praxis_home.clone())
            .write_value(ConfigValueWriteParams {
                key_path: format!("plugins.{}.enabled", plugin_id.as_key()),
                value: json!(enabled),
                merge_strategy: MergeStrategy::Replace,
                file_path: None,
                expected_version: None,
            })
            .await
            .map(|_| ())
            .map_err(PluginSetEnabledError::from)?;
        self.clear_cache();
        Ok(activation_delta)
    }

    async fn uninstall_plugin_id(
        &self,
        plugin_id: PluginId,
    ) -> Result<PluginActivationDelta<McpServerConfig>, PluginUninstallError> {
        let activation_delta = self
            .store
            .active_plugin_root(&plugin_id)
            .map(|plugin_root| plugin_activation_delta_from_root(&plugin_id, plugin_root.as_path()))
            .unwrap_or_else(|| PluginActivationDelta {
                plugin_id: Some(plugin_id.clone()),
                ..PluginActivationDelta::default()
            });
        let plugin_telemetry = self
            .store
            .active_plugin_root(&plugin_id)
            .map(|_| installed_plugin_telemetry_metadata(self.praxis_home.as_path(), &plugin_id));
        let store = self.store.clone();
        let plugin_id_for_store = plugin_id.clone();
        tokio::task::spawn_blocking(move || store.uninstall(&plugin_id_for_store))
            .await
            .map_err(PluginUninstallError::join)??;

        ConfigEditsBuilder::new(&self.praxis_home)
            .with_edits([ConfigEdit::ClearPath {
                segments: vec!["plugins".to_string(), plugin_id.as_key()],
            }])
            .apply()
            .await?;

        let analytics_events_client = match self.analytics_events_client.read() {
            Ok(client) => client.clone(),
            Err(err) => err.into_inner().clone(),
        };
        if let Some(plugin_telemetry) = plugin_telemetry
            && let Some(analytics_events_client) = analytics_events_client
        {
            analytics_events_client.track_plugin_uninstalled(plugin_telemetry);
        }

        Ok(activation_delta)
    }

    pub async fn sync_plugins_from_remote(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
        additive_only: bool,
    ) -> Result<RemotePluginSyncResult, PluginRemoteSyncError> {
        let _remote_sync_guard = self.remote_sync_lock.lock().await;

        if !config.features.enabled(Feature::Plugins) {
            return Ok(RemotePluginSyncResult::default());
        }

        info!("starting remote plugin sync");
        let remote_plugins = fetch_remote_plugin_status(config, auth)
            .await
            .map_err(PluginRemoteSyncError::from)?;
        let configured_plugins = configured_plugins_from_stack(&config.config_layer_stack);
        let curated_marketplace_root = curated_plugins_repo_path(self.praxis_home.as_path());
        let curated_marketplace_path = AbsolutePathBuf::try_from(
            curated_marketplace_root.join(".agents/plugins/marketplace.json"),
        )
        .map_err(|_| PluginRemoteSyncError::LocalMarketplaceNotFound)?;
        let curated_marketplace = match load_marketplace(&curated_marketplace_path) {
            Ok(marketplace) => marketplace,
            Err(MarketplaceError::MarketplaceNotFound { .. }) => {
                return Err(PluginRemoteSyncError::LocalMarketplaceNotFound);
            }
            Err(err) => return Err(err.into()),
        };

        let marketplace_name = curated_marketplace.name.clone();
        let curated_plugin_version = read_curated_plugins_sha(self.praxis_home.as_path())
            .ok_or_else(|| {
                PluginStoreError::Invalid(
                    "local curated marketplace sha is not available".to_string(),
                )
            })?;
        let mut local_plugins = Vec::<(
            String,
            PluginId,
            AbsolutePathBuf,
            Option<bool>,
            Option<String>,
            bool,
        )>::new();
        let mut local_plugin_names = HashSet::new();
        for plugin in curated_marketplace.plugins {
            let plugin_name = plugin.name;
            if !local_plugin_names.insert(plugin_name.clone()) {
                warn!(
                    plugin = plugin_name,
                    marketplace = %marketplace_name,
                    "ignoring duplicate local plugin entry during remote sync"
                );
                continue;
            }

            let plugin_id = PluginId::new(plugin_name.clone(), marketplace_name.clone())?;
            let plugin_key = plugin_id.as_key();
            let source_path = match plugin.source {
                MarketplacePluginSource::Local { path } => path,
            };
            let current_enabled = configured_plugins
                .get(&plugin_key)
                .map(|plugin| plugin.enabled);
            let installed_version = self.store.active_plugin_version(&plugin_id);
            let product_allowed =
                self.restriction_product_matches(plugin.policy.products.as_deref());
            local_plugins.push((
                plugin_name,
                plugin_id,
                source_path,
                current_enabled,
                installed_version,
                product_allowed,
            ));
        }

        let mut remote_installed_plugin_names = HashSet::<String>::new();
        for plugin in remote_plugins {
            if plugin.marketplace_name != marketplace_name {
                return Err(PluginRemoteSyncError::UnknownRemoteMarketplace {
                    marketplace_name: plugin.marketplace_name,
                });
            }
            if !local_plugin_names.contains(&plugin.name) {
                warn!(
                    plugin = plugin.name,
                    marketplace = %marketplace_name,
                    "ignoring remote plugin missing from local marketplace during sync"
                );
                continue;
            }
            // For now, sync treats remote `enabled = false` as uninstall rather than a distinct
            // disabled state.
            // TODO: Switch sync to `plugins/installed` so install and enable states stay distinct.
            if !plugin.enabled {
                continue;
            }
            if !remote_installed_plugin_names.insert(plugin.name.clone()) {
                return Err(PluginRemoteSyncError::DuplicateRemotePlugin {
                    plugin_name: plugin.name,
                });
            }
        }

        let mut config_edits = Vec::new();
        let mut installs = Vec::new();
        let mut uninstalls = Vec::new();
        let mut result = RemotePluginSyncResult::default();
        let remote_plugin_count = remote_installed_plugin_names.len();
        let local_plugin_count = local_plugins.len();

        for (
            plugin_name,
            plugin_id,
            source_path,
            current_enabled,
            installed_version,
            product_allowed,
        ) in local_plugins
        {
            let plugin_key = plugin_id.as_key();
            let is_installed = installed_version.is_some();
            if !product_allowed {
                continue;
            }
            if remote_installed_plugin_names.contains(&plugin_name) {
                if !is_installed {
                    installs.push((
                        source_path,
                        plugin_id.clone(),
                        curated_plugin_version.clone(),
                    ));
                }
                if !is_installed {
                    result.installed_plugin_ids.push(plugin_key.clone());
                }

                if current_enabled != Some(true) {
                    result.enabled_plugin_ids.push(plugin_key.clone());
                    config_edits.push(ConfigEdit::SetPath {
                        segments: vec!["plugins".to_string(), plugin_key, "enabled".to_string()],
                        value: value(true),
                    });
                }
            } else if !additive_only {
                if is_installed {
                    uninstalls.push(plugin_id);
                }
                if is_installed || current_enabled.is_some() {
                    result.uninstalled_plugin_ids.push(plugin_key.clone());
                }
                if current_enabled.is_some() {
                    config_edits.push(ConfigEdit::ClearPath {
                        segments: vec!["plugins".to_string(), plugin_key],
                    });
                }
            }
        }

        let store = self.store.clone();
        let store_result = tokio::task::spawn_blocking(move || {
            for (source_path, plugin_id, plugin_version) in installs {
                store.install_with_version(source_path, plugin_id, plugin_version)?;
            }
            for plugin_id in uninstalls {
                store.uninstall(&plugin_id)?;
            }
            Ok::<(), PluginStoreError>(())
        })
        .await
        .map_err(PluginRemoteSyncError::join)?;
        if let Err(err) = store_result {
            self.clear_cache();
            return Err(err.into());
        }

        let config_result = if config_edits.is_empty() {
            Ok(())
        } else {
            ConfigEditsBuilder::new(&self.praxis_home)
                .with_edits(config_edits)
                .apply()
                .await
        };
        self.clear_cache();
        config_result?;

        info!(
            marketplace = %marketplace_name,
            remote_plugin_count,
            local_plugin_count,
            installed_plugin_ids = ?result.installed_plugin_ids,
            enabled_plugin_ids = ?result.enabled_plugin_ids,
            disabled_plugin_ids = ?result.disabled_plugin_ids,
            uninstalled_plugin_ids = ?result.uninstalled_plugin_ids,
            "completed remote plugin sync"
        );

        Ok(result)
    }

    pub async fn sync_marketplace_provider(
        &self,
        config: &Config,
        marketplace_name: String,
    ) -> Result<PluginMarketplaceSyncOutcome, PluginMarketplaceProviderSyncError> {
        let marketplace = config
            .plugin_marketplaces
            .get(&marketplace_name)
            .ok_or_else(|| {
                PluginMarketplaceProviderSyncError::NotConfigured(marketplace_name.clone())
            })?;
        if !marketplace.enabled {
            return Err(PluginMarketplaceProviderSyncError::Disabled(
                marketplace_name,
            ));
        }

        let outcome = match &marketplace.provider {
            PluginMarketplaceProviderConfig::Local { path } => PluginMarketplaceSyncOutcome {
                marketplace_name,
                changed: false,
                local_root: Some(path.as_path().to_path_buf()),
                version: None,
                diagnostics: Vec::new(),
            },
            PluginMarketplaceProviderConfig::Git {
                repo,
                reference,
                path,
            } => {
                if praxis_plugin::validate_plugin_segment(&marketplace_name, "marketplace name")
                    .is_err()
                {
                    return Err(PluginMarketplaceProviderSyncError::InvalidMarketplaceName(
                        marketplace_name,
                    ));
                }
                let praxis_home = self.praxis_home.clone();
                let marketplace_name_for_task = marketplace_name.clone();
                let repo = repo.clone();
                let reference = reference.clone();
                let path = path.clone();
                tokio::task::spawn_blocking(move || {
                    sync_git_marketplace_provider(
                        praxis_home.as_path(),
                        &marketplace_name_for_task,
                        &repo,
                        reference.as_deref(),
                        path.as_deref(),
                    )
                })
                .await
                .map_err(PluginMarketplaceProviderSyncError::from)?
                .map_err(|message| PluginMarketplaceProviderSyncError::Git {
                    marketplace_name,
                    message,
                })?
            }
            PluginMarketplaceProviderConfig::Http { .. } => {
                return Err(PluginMarketplaceProviderSyncError::UnsupportedProvider {
                    marketplace_name,
                    provider: "http",
                });
            }
        };

        self.clear_cache();
        Ok(outcome)
    }

    pub fn list_marketplaces_for_config(
        &self,
        config: &Config,
        additional_roots: &[AbsolutePathBuf],
    ) -> Result<ConfiguredMarketplaceListOutcome, MarketplaceError> {
        if !config.features.enabled(Feature::Plugins) {
            return Ok(ConfiguredMarketplaceListOutcome::default());
        }

        let (installed_plugins, enabled_plugins) = self.configured_plugin_states(config);
        let marketplace_outcome =
            list_marketplaces(&self.marketplace_roots(config, additional_roots))?;
        let mut seen_plugin_keys = HashSet::new();
        let marketplaces = marketplace_outcome
            .marketplaces
            .into_iter()
            .filter_map(|marketplace| {
                let marketplace_name = marketplace.name.clone();
                let plugins = marketplace
                    .plugins
                    .into_iter()
                    .filter_map(|plugin| {
                        let plugin_key = format!("{}@{marketplace_name}", plugin.name);
                        if !seen_plugin_keys.insert(plugin_key.clone()) {
                            return None;
                        }
                        if !self.restriction_product_matches(plugin.policy.products.as_deref()) {
                            return None;
                        }

                        Some(ConfiguredMarketplacePlugin {
                            // Enabled state is keyed by `<plugin>@<marketplace>`, so duplicate
                            // plugin entries from duplicate marketplace files intentionally
                            // resolve to the first discovered source.
                            id: plugin_key.clone(),
                            installed: installed_plugins.contains(&plugin_key),
                            enabled: enabled_plugins.contains(&plugin_key),
                            name: plugin.name,
                            source: plugin.source,
                            policy: plugin.policy,
                            interface: plugin.interface,
                            llm: plugin.llm,
                        })
                    })
                    .collect::<Vec<_>>();

                (!plugins.is_empty()).then_some(ConfiguredMarketplace {
                    name: marketplace.name,
                    path: marketplace.path,
                    interface: if marketplace_name == OPENAI_CURATED_MARKETPLACE_NAME {
                        Some(MarketplaceInterface {
                            display_name: Some(OPENAI_CURATED_MARKETPLACE_DISPLAY_NAME.to_string()),
                        })
                    } else {
                        marketplace.interface
                    },
                    plugins,
                })
            })
            .collect();

        Ok(ConfiguredMarketplaceListOutcome {
            marketplaces,
            errors: marketplace_outcome.errors,
        })
    }

    pub fn read_plugin_for_config(
        &self,
        config: &Config,
        request: &PluginReadRequest,
    ) -> Result<PluginReadOutcome, MarketplaceError> {
        if !config.features.enabled(Feature::Plugins) {
            return Err(MarketplaceError::PluginsDisabled);
        }

        let marketplace = load_marketplace(&request.marketplace_path)?;
        let marketplace_name = marketplace.name.clone();
        let plugin = marketplace
            .plugins
            .into_iter()
            .find(|plugin| plugin.name == request.plugin_name);
        let Some(plugin) = plugin else {
            return Err(MarketplaceError::PluginNotFound {
                plugin_name: request.plugin_name.clone(),
                marketplace_name,
            });
        };
        if !self.restriction_product_matches(plugin.policy.products.as_deref()) {
            return Err(MarketplaceError::PluginNotFound {
                plugin_name: request.plugin_name.clone(),
                marketplace_name,
            });
        }

        let plugin_id = PluginId::new(plugin.name.clone(), marketplace.name.clone()).map_err(
            |err| match err {
                PluginIdError::Invalid(message) => MarketplaceError::InvalidPlugin(message),
            },
        )?;
        let plugin_key = plugin_id.as_key();
        let (installed_plugins, enabled_plugins) = self.configured_plugin_states(config);
        let source_path = match &plugin.source {
            MarketplacePluginSource::Local { path } => path.clone(),
        };
        if !source_path.as_path().is_dir() {
            return Err(MarketplaceError::InvalidPlugin(
                "path does not exist or is not a directory".to_string(),
            ));
        }
        let manifest = load_plugin_manifest(source_path.as_path()).ok_or_else(|| {
            MarketplaceError::InvalidPlugin(
                "missing or invalid .praxis-plugin/plugin.json".to_string(),
            )
        })?;
        let description = manifest.description.clone();
        let manifest_paths = &manifest.paths;
        let skill_config_rules = skill_config_rules_from_stack(&config.config_layer_stack);
        let resolved_skills = load_plugin_skills(
            source_path.as_path(),
            manifest_paths,
            self.restriction_product,
            &skill_config_rules,
        );
        let apps = load_plugin_apps(source_path.as_path());
        let mcp_config_paths = plugin_mcp_config_paths(source_path.as_path(), manifest_paths);
        let mut mcp_server_names = Vec::new();
        for mcp_config_path in mcp_config_paths {
            mcp_server_names.extend(
                load_mcp_servers_from_file(source_path.as_path(), &mcp_config_path)
                    .mcp_servers
                    .into_keys(),
            );
        }
        mcp_server_names.sort_unstable();
        mcp_server_names.dedup();

        Ok(PluginReadOutcome {
            marketplace_name: if marketplace.name == OPENAI_CURATED_MARKETPLACE_NAME {
                OPENAI_CURATED_MARKETPLACE_DISPLAY_NAME.to_string()
            } else {
                marketplace.name
            },
            marketplace_path: marketplace.path,
            plugin: PluginDetail {
                id: plugin_key.clone(),
                name: plugin.name,
                description,
                source: plugin.source,
                policy: plugin.policy,
                interface: plugin.interface,
                llm: manifest.llm,
                installed: installed_plugins.contains(&plugin_key),
                enabled: enabled_plugins.contains(&plugin_key),
                skills: resolved_skills.skills,
                disabled_skill_paths: resolved_skills.disabled_skill_paths,
                apps,
                mcp_server_names,
            },
        })
    }

    pub fn maybe_start_plugin_startup_tasks_for_config(
        self: &Arc<Self>,
        config: &Config,
        auth_manager: Arc<AuthManager>,
    ) {
        if config.features.enabled(Feature::Plugins) {
            self.start_curated_repo_sync();
            start_startup_remote_plugin_sync_once(
                Arc::clone(self),
                self.praxis_home.clone(),
                config.clone(),
                auth_manager.clone(),
            );

            let config = config.clone();
            let manager = Arc::clone(self);
            tokio::spawn(async move {
                let auth = auth_manager.auth().await;
                if let Err(err) = manager
                    .featured_plugin_ids_for_config(&config, auth.as_ref())
                    .await
                {
                    warn!(
                        error = %err,
                        "failed to warm featured plugin ids cache"
                    );
                }
            });
        }
    }

    fn start_curated_repo_sync(self: &Arc<Self>) {
        if CURATED_REPO_SYNC_STARTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let manager = Arc::clone(self);
        let praxis_home = self.praxis_home.clone();
        if let Err(err) = std::thread::Builder::new()
            .name("plugins-curated-repo-sync".to_string())
            .spawn(
                move || match sync_openai_plugins_repo(praxis_home.as_path()) {
                    Ok(curated_plugin_version) => {
                        let configured_curated_plugin_ids =
                            configured_curated_plugin_ids_from_praxis_home(praxis_home.as_path());
                        match refresh_curated_plugin_cache(
                            praxis_home.as_path(),
                            &curated_plugin_version,
                            &configured_curated_plugin_ids,
                        ) {
                            Ok(cache_refreshed) => {
                                if cache_refreshed {
                                    manager.clear_cache();
                                }
                            }
                            Err(err) => {
                                manager.clear_cache();
                                CURATED_REPO_SYNC_STARTED.store(false, Ordering::SeqCst);
                                warn!("failed to refresh curated plugin cache after sync: {err}");
                            }
                        }
                    }
                    Err(err) => {
                        CURATED_REPO_SYNC_STARTED.store(false, Ordering::SeqCst);
                        warn!("failed to sync curated plugins repo: {err}");
                    }
                },
            )
        {
            CURATED_REPO_SYNC_STARTED.store(false, Ordering::SeqCst);
            warn!("failed to start curated plugins repo sync task: {err}");
        }
    }

    fn configured_plugin_states(&self, config: &Config) -> (HashSet<String>, HashSet<String>) {
        let configured_plugins = configured_plugins_from_stack(&config.config_layer_stack);
        let installed_plugins = configured_plugins
            .keys()
            .filter(|plugin_key| {
                PluginId::parse(plugin_key)
                    .ok()
                    .is_some_and(|plugin_id| self.store.is_installed(&plugin_id))
            })
            .cloned()
            .collect::<HashSet<_>>();
        let enabled_plugins = configured_plugins
            .into_iter()
            .filter_map(|(plugin_key, plugin)| plugin.enabled.then_some(plugin_key))
            .collect::<HashSet<_>>();
        (installed_plugins, enabled_plugins)
    }

    fn marketplace_roots(
        &self,
        config: &Config,
        additional_roots: &[AbsolutePathBuf],
    ) -> Vec<AbsolutePathBuf> {
        let mut roots = additional_roots.to_vec();
        let curated_repo_root = curated_plugins_repo_path(self.praxis_home.as_path());
        if curated_repo_root.is_dir()
            && let Ok(curated_repo_root) = AbsolutePathBuf::try_from(curated_repo_root)
        {
            roots.push(curated_repo_root);
        }
        roots.extend(self.configured_marketplace_roots(config));
        roots.sort_unstable_by(|left, right| left.as_path().cmp(right.as_path()));
        roots.dedup();
        roots
    }

    fn configured_marketplace_roots(&self, config: &Config) -> Vec<AbsolutePathBuf> {
        let mut roots = Vec::new();
        for (name, marketplace) in &config.plugin_marketplaces {
            if !marketplace.enabled {
                continue;
            }

            match &marketplace.provider {
                PluginMarketplaceProviderConfig::Local { path } => roots.push(path.clone()),
                PluginMarketplaceProviderConfig::Git { path, .. } => {
                    if let Some(root) = self.cached_marketplace_root(name, path.as_deref()) {
                        roots.push(root);
                    }
                }
                PluginMarketplaceProviderConfig::Http { .. } => {
                    if let Some(root) = self.cached_marketplace_root(name, None) {
                        roots.push(root);
                    }
                }
            }
        }
        roots
    }

    fn cached_marketplace_root(
        &self,
        marketplace_name: &str,
        relative_path: Option<&Path>,
    ) -> Option<AbsolutePathBuf> {
        if praxis_plugin::validate_plugin_segment(marketplace_name, "marketplace name").is_err() {
            warn!(
                marketplace = marketplace_name,
                "ignoring invalid plugin marketplace cache name"
            );
            return None;
        }
        let mut root = self
            .praxis_home
            .join(MARKETPLACE_PROVIDER_CACHE_DIR)
            .join(marketplace_name);
        if let Some(relative_path) = relative_path {
            root.push(relative_path);
        }
        root.is_dir()
            .then(|| AbsolutePathBuf::try_from(root).ok())
            .flatten()
    }

    pub fn configured_marketplace_refs(&self, config: &Config) -> Vec<PluginMarketplaceRef> {
        let mut refs = config
            .plugin_marketplaces
            .iter()
            .map(|(name, marketplace)| PluginMarketplaceRef {
                name: name.clone(),
                display_name: marketplace.display_name.clone(),
                provider: marketplace_provider_source(&marketplace.provider),
                enabled: marketplace.enabled,
                sync_on_startup: marketplace.sync_on_startup,
            })
            .collect::<Vec<_>>();
        refs.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        refs
    }
}

fn marketplace_provider_source(
    provider: &PluginMarketplaceProviderConfig,
) -> PluginMarketplaceProviderSource {
    match provider {
        PluginMarketplaceProviderConfig::Local { path } => PluginMarketplaceProviderSource::Local {
            path: path.as_path().to_path_buf(),
        },
        PluginMarketplaceProviderConfig::Git {
            repo,
            reference,
            path,
        } => PluginMarketplaceProviderSource::Git {
            repo: repo.clone(),
            reference: reference.clone(),
            path: path.clone(),
        },
        PluginMarketplaceProviderConfig::Http { url } => {
            PluginMarketplaceProviderSource::Http { url: url.clone() }
        }
    }
}

fn sync_git_marketplace_provider(
    praxis_home: &Path,
    marketplace_name: &str,
    repo: &str,
    reference: Option<&str>,
    path: Option<&Path>,
) -> Result<PluginMarketplaceSyncOutcome, String> {
    let relative_path = validate_marketplace_provider_relative_path(path)?;
    let repo_path = marketplace_provider_cache_root(praxis_home, marketplace_name);
    let old_version = if repo_path.join(".git").is_dir() {
        git_head_sha(repo_path.as_path(), "git").ok()
    } else {
        None
    };

    let staged_repo_dir = prepare_marketplace_provider_temp_dir(repo_path.as_path())?;
    clone_git_marketplace_repo(repo, reference, staged_repo_dir.path(), marketplace_name)?;
    ensure_marketplace_provider_manifest_exists(staged_repo_dir.path(), relative_path.as_deref())?;
    let new_version = git_head_sha(staged_repo_dir.path(), "git").ok();

    let changed = old_version != new_version;
    if changed || !repo_path.is_dir() {
        activate_marketplace_provider_repo(repo_path.as_path(), staged_repo_dir)?;
    }

    let local_root = relative_path
        .as_deref()
        .map(|path| repo_path.join(path))
        .unwrap_or(repo_path);

    Ok(PluginMarketplaceSyncOutcome {
        marketplace_name: marketplace_name.to_string(),
        changed,
        local_root: Some(local_root),
        version: new_version,
        diagnostics: Vec::new(),
    })
}

fn marketplace_provider_cache_root(praxis_home: &Path, marketplace_name: &str) -> PathBuf {
    praxis_home
        .join(MARKETPLACE_PROVIDER_CACHE_DIR)
        .join(marketplace_name)
}

fn validate_marketplace_provider_relative_path(
    path: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if path.as_os_str().is_empty() || path == Path::new(".") {
        return Ok(None);
    }
    if path.is_absolute() {
        return Err(format!(
            "git marketplace subpath must be relative, got {}",
            path.display()
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "git marketplace subpath must stay within the repository root, got {}",
            path.display()
        ));
    }
    Ok(Some(path.to_path_buf()))
}

fn prepare_marketplace_provider_temp_dir(repo_path: &Path) -> Result<TempDir, String> {
    let Some(parent) = repo_path.parent() else {
        return Err(format!(
            "failed to determine marketplace cache parent directory for {}",
            repo_path.display()
        ));
    };
    fs::create_dir_all(parent).map_err(|err| {
        format!(
            "failed to create marketplace cache parent directory {}: {err}",
            parent.display()
        )
    })?;
    remove_stale_marketplace_provider_temp_dirs(parent);
    tempfile::Builder::new()
        .prefix("marketplace-clone-")
        .tempdir_in(parent)
        .map_err(|err| {
            format!(
                "failed to create temporary marketplace clone directory in {}: {err}",
                parent.display()
            )
        })
}

fn remove_stale_marketplace_provider_temp_dirs(parent: &Path) {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(err) => {
            warn!(
                error = %err,
                parent = %parent.display(),
                "failed to list marketplace cache parent for stale cleanup"
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        let is_temp_dir = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("marketplace-clone-"));
        if !is_temp_dir {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified
            .elapsed()
            .is_ok_and(|age| age >= MARKETPLACE_PROVIDER_STALE_TEMP_DIR_MAX_AGE)
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn clone_git_marketplace_repo(
    repo: &str,
    reference: Option<&str>,
    destination: &Path,
    marketplace_name: &str,
) -> Result<(), String> {
    let repo_url = normalize_git_marketplace_repo(repo);
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("clone")
        .arg("--depth")
        .arg("1");
    if let Some(reference) = reference.filter(|reference| !reference.trim().is_empty()) {
        command.arg("--branch").arg(reference);
    }
    command.arg(repo_url).arg(destination);

    let output = run_git_command_with_timeout(
        &mut command,
        &format!("git clone plugin marketplace `{marketplace_name}`"),
        MARKETPLACE_PROVIDER_GIT_TIMEOUT,
    )?;
    ensure_git_success(&output, "git clone plugin marketplace")
}

fn normalize_git_marketplace_repo(repo: &str) -> String {
    let trimmed = repo.trim();
    if trimmed.contains("://") || trimmed.starts_with("git@") || trimmed.ends_with(".git") {
        return trimmed.to_string();
    }
    let mut parts = trimmed.split('/');
    if let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next())
        && !owner.is_empty()
        && !name.is_empty()
    {
        return format!("https://github.com/{owner}/{name}.git");
    }
    trimmed.to_string()
}

fn ensure_marketplace_provider_manifest_exists(
    repo_path: &Path,
    relative_path: Option<&Path>,
) -> Result<(), String> {
    let root = relative_path
        .map(|path| repo_path.join(path))
        .unwrap_or_else(|| repo_path.to_path_buf());
    let manifest = root.join(".agents/plugins/marketplace.json");
    if manifest.is_file() {
        return Ok(());
    }
    Err(format!(
        "plugin marketplace repo missing manifest at {}",
        manifest.display()
    ))
}

fn activate_marketplace_provider_repo(
    repo_path: &Path,
    staged_repo_dir: TempDir,
) -> Result<(), String> {
    let staged_repo_path = staged_repo_dir.path();
    if repo_path.exists() {
        let parent = repo_path.parent().ok_or_else(|| {
            format!(
                "failed to determine marketplace cache parent directory for {}",
                repo_path.display()
            )
        })?;
        let backup_dir = tempfile::Builder::new()
            .prefix("marketplace-backup-")
            .tempdir_in(parent)
            .map_err(|err| {
                format!(
                    "failed to create marketplace cache backup directory in {}: {err}",
                    parent.display()
                )
            })?;
        let backup_repo_path = backup_dir.path().join("repo");
        fs::rename(repo_path, &backup_repo_path).map_err(|err| {
            format!(
                "failed to move previous marketplace cache out of the way at {}: {err}",
                repo_path.display()
            )
        })?;
        if let Err(err) = fs::rename(staged_repo_path, repo_path) {
            let rollback_result = fs::rename(&backup_repo_path, repo_path);
            return match rollback_result {
                Ok(()) => Err(format!(
                    "failed to activate new marketplace cache at {}: {err}",
                    repo_path.display()
                )),
                Err(rollback_err) => {
                    let backup_path = backup_dir.keep().join("repo");
                    Err(format!(
                        "failed to activate new marketplace cache at {}: {err}; failed to restore previous cache (left at {}): {rollback_err}",
                        repo_path.display(),
                        backup_path.display()
                    ))
                }
            };
        }
    } else {
        fs::rename(staged_repo_path, repo_path).map_err(|err| {
            format!(
                "failed to activate marketplace cache at {}: {err}",
                repo_path.display()
            )
        })?;
    }
    Ok(())
}

fn git_head_sha(repo_path: &Path, git_binary: &str) -> Result<String, String> {
    let output = Command::new(git_binary)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(repo_path)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|err| {
            format!(
                "failed to run git rev-parse HEAD in {}: {err}",
                repo_path.display()
            )
        })?;
    ensure_git_success(&output, "git rev-parse HEAD")?;

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!(
            "git rev-parse HEAD returned empty output in {}",
            repo_path.display()
        ));
    }
    Ok(sha)
}

fn run_git_command_with_timeout(
    command: &mut Command,
    context: &str,
    timeout: Duration,
) -> Result<Output, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to run {context}: {err}"))?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|err| format!("failed to wait for {context}: {err}"));
            }
            Ok(None) => {}
            Err(err) => return Err(format!("failed to poll {context}: {err}")),
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|err| format!("failed to wait for {context} after timeout: {err}"))?;
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return if stderr.is_empty() {
                Err(format!("{context} timed out after {}s", timeout.as_secs()))
            } else {
                Err(format!(
                    "{context} timed out after {}s: {stderr}",
                    timeout.as_secs()
                ))
            };
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn ensure_git_success(output: &Output, context: &str) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("{context} failed with status {}", output.status))
    } else {
        Err(format!(
            "{context} failed with status {}: {stderr}",
            output.status
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginInstallError {
    #[error("{0}")]
    Marketplace(#[from] MarketplaceError),

    #[error("{0}")]
    Remote(#[from] RemotePluginMutationError),

    #[error("{0}")]
    Store(#[from] PluginStoreError),

    #[error("{0}")]
    Config(#[from] ConfigServiceError),

    #[error("failed to join plugin install task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl PluginInstallError {
    fn join(source: tokio::task::JoinError) -> Self {
        Self::Join(source)
    }

    pub fn is_invalid_request(&self) -> bool {
        matches!(
            self,
            Self::Marketplace(
                MarketplaceError::MarketplaceNotFound { .. }
                    | MarketplaceError::InvalidMarketplaceFile { .. }
                    | MarketplaceError::PluginNotFound { .. }
                    | MarketplaceError::PluginNotAvailable { .. }
                    | MarketplaceError::InvalidPlugin(_)
            ) | Self::Store(PluginStoreError::Invalid(_))
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginUninstallError {
    #[error("{0}")]
    InvalidPluginId(#[from] PluginIdError),

    #[error("{0}")]
    Remote(#[from] RemotePluginMutationError),

    #[error("{0}")]
    Store(#[from] PluginStoreError),

    #[error("{0}")]
    Config(#[from] anyhow::Error),

    #[error("failed to join plugin uninstall task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl PluginUninstallError {
    fn join(source: tokio::task::JoinError) -> Self {
        Self::Join(source)
    }

    pub fn is_invalid_request(&self) -> bool {
        matches!(self, Self::InvalidPluginId(_))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginSetEnabledError {
    #[error("{0}")]
    InvalidPluginId(#[from] PluginIdError),

    #[error("plugin `{0}` is not installed")]
    NotInstalled(String),

    #[error("{0}")]
    Config(#[from] ConfigServiceError),
}

impl PluginSetEnabledError {
    pub fn is_invalid_request(&self) -> bool {
        matches!(self, Self::InvalidPluginId(_) | Self::NotInstalled(_))
    }
}

fn log_plugin_load_errors(outcome: &PluginLoadOutcome) {
    for plugin in outcome
        .plugins()
        .iter()
        .filter(|plugin| plugin.error.is_some())
    {
        if let Some(error) = plugin.error.as_deref() {
            warn!(
                plugin = plugin.config_name,
                path = %plugin.root.display(),
                "failed to load plugin: {error}"
            );
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginMcpFile {
    #[serde(default)]
    mcp_servers: HashMap<String, JsonValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginAppFile {
    #[serde(default)]
    apps: HashMap<String, PluginAppConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct PluginAppConfig {
    id: String,
}

pub(crate) fn load_plugins_from_layer_stack(
    config_layer_stack: &ConfigLayerStack,
    store: &PluginStore,
    restriction_product: Option<Product>,
) -> PluginLoadOutcome {
    let skill_config_rules = skill_config_rules_from_stack(config_layer_stack);
    let mut configured_plugins: Vec<_> = configured_plugins_from_stack(config_layer_stack)
        .into_iter()
        .collect();
    configured_plugins.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

    let mut plugins = Vec::with_capacity(configured_plugins.len());
    let mut seen_mcp_server_names = HashMap::<String, String>::new();
    for (configured_name, plugin) in configured_plugins {
        let loaded_plugin = load_plugin(
            configured_name.clone(),
            &plugin,
            store,
            restriction_product,
            &skill_config_rules,
        );
        for name in loaded_plugin.mcp_servers.keys() {
            if let Some(previous_plugin) =
                seen_mcp_server_names.insert(name.clone(), configured_name.clone())
            {
                warn!(
                    plugin = configured_name,
                    previous_plugin,
                    server = name,
                    "skipping duplicate plugin MCP server name"
                );
            }
        }
        plugins.push(loaded_plugin);
    }

    PluginLoadOutcome::from_plugins(plugins)
}

fn refresh_curated_plugin_cache(
    praxis_home: &Path,
    plugin_version: &str,
    configured_curated_plugin_ids: &[PluginId],
) -> Result<bool, String> {
    let store = PluginStore::new(praxis_home.to_path_buf());
    let curated_marketplace_path = AbsolutePathBuf::try_from(
        curated_plugins_repo_path(praxis_home).join(".agents/plugins/marketplace.json"),
    )
    .map_err(|_| "local curated marketplace is not available".to_string())?;
    let curated_marketplace = load_marketplace(&curated_marketplace_path)
        .map_err(|err| format!("failed to load curated marketplace for cache refresh: {err}"))?;

    let mut plugin_sources = HashMap::<String, AbsolutePathBuf>::new();
    for plugin in curated_marketplace.plugins {
        let plugin_name = plugin.name;
        if plugin_sources.contains_key(&plugin_name) {
            warn!(
                plugin = plugin_name,
                marketplace = OPENAI_CURATED_MARKETPLACE_NAME,
                "ignoring duplicate curated plugin entry during cache refresh"
            );
            continue;
        }
        let source_path = match plugin.source {
            MarketplacePluginSource::Local { path } => path,
        };
        plugin_sources.insert(plugin_name, source_path);
    }

    let mut cache_refreshed = false;
    for plugin_id in configured_curated_plugin_ids {
        if store.active_plugin_version(plugin_id).as_deref() == Some(plugin_version) {
            continue;
        }

        let Some(source_path) = plugin_sources.get(&plugin_id.plugin_name).cloned() else {
            warn!(
                plugin = plugin_id.plugin_name,
                marketplace = OPENAI_CURATED_MARKETPLACE_NAME,
                "configured curated plugin no longer exists in curated marketplace during cache refresh"
            );
            continue;
        };

        store
            .install_with_version(source_path, plugin_id.clone(), plugin_version.to_string())
            .map_err(|err| {
                format!(
                    "failed to refresh curated plugin cache for {}: {err}",
                    plugin_id.as_key()
                )
            })?;
        cache_refreshed = true;
    }

    Ok(cache_refreshed)
}

fn configured_plugins_from_stack(
    config_layer_stack: &ConfigLayerStack,
) -> HashMap<String, PluginConfig> {
    // Plugin entries remain persisted user config only.
    let Some(user_layer) = config_layer_stack.get_user_layer() else {
        return HashMap::new();
    };
    configured_plugins_from_user_config_value(&user_layer.config)
}

fn configured_plugins_from_user_config_value(
    user_config: &toml::Value,
) -> HashMap<String, PluginConfig> {
    let Some(plugins_value) = user_config.get("plugins") else {
        return HashMap::new();
    };
    match plugins_value.clone().try_into() {
        Ok(plugins) => plugins,
        Err(err) => {
            warn!("invalid plugins config: {err}");
            HashMap::new()
        }
    }
}

fn configured_curated_plugin_ids(
    configured_plugins: HashMap<String, PluginConfig>,
) -> Vec<PluginId> {
    let mut configured_curated_plugin_ids = configured_plugins
        .into_keys()
        .filter_map(|plugin_key| match PluginId::parse(&plugin_key) {
            Ok(plugin_id) if plugin_id.marketplace_name == OPENAI_CURATED_MARKETPLACE_NAME => {
                Some(plugin_id)
            }
            Ok(_) => None,
            Err(err) => {
                warn!(
                    plugin_key,
                    error = %err,
                    "ignoring invalid configured plugin key during curated sync setup"
                );
                None
            }
        })
        .collect::<Vec<_>>();
    configured_curated_plugin_ids.sort_unstable_by_key(PluginId::as_key);
    configured_curated_plugin_ids
}

fn configured_curated_plugin_ids_from_praxis_home(praxis_home: &Path) -> Vec<PluginId> {
    let config_path = praxis_home.join(CONFIG_TOML_FILE);
    let user_config = match fs::read_to_string(&config_path) {
        Ok(user_config) => user_config,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            warn!(
                path = %config_path.display(),
                error = %err,
                "failed to read user config while refreshing curated plugin cache"
            );
            return Vec::new();
        }
    };

    let user_config = match toml::from_str::<toml::Value>(&user_config) {
        Ok(user_config) => user_config,
        Err(err) => {
            warn!(
                path = %config_path.display(),
                error = %err,
                "failed to parse user config while refreshing curated plugin cache"
            );
            return Vec::new();
        }
    };

    configured_curated_plugin_ids(configured_plugins_from_user_config_value(&user_config))
}

fn load_plugin(
    config_name: String,
    plugin: &PluginConfig,
    store: &PluginStore,
    restriction_product: Option<Product>,
    skill_config_rules: &SkillConfigRules,
) -> LoadedPlugin {
    let plugin_id = PluginId::parse(&config_name);
    let active_plugin_root = plugin_id
        .as_ref()
        .ok()
        .and_then(|plugin_id| store.active_plugin_root(plugin_id));
    let root = active_plugin_root
        .clone()
        .unwrap_or_else(|| match &plugin_id {
            Ok(plugin_id) => store.plugin_base_root(plugin_id),
            Err(_) => store.root().clone(),
        });
    let mut loaded_plugin = LoadedPlugin {
        config_name,
        manifest_name: None,
        manifest_description: None,
        root,
        enabled: plugin.enabled,
        skill_roots: Vec::new(),
        disabled_skill_paths: HashSet::new(),
        has_enabled_skills: false,
        mcp_servers: HashMap::new(),
        apps: Vec::new(),
        llm: None,
        error: None,
    };

    if !plugin.enabled {
        return loaded_plugin;
    }

    let plugin_root = match plugin_id {
        Ok(_) => match active_plugin_root {
            Some(plugin_root) => plugin_root,
            None => {
                loaded_plugin.error = Some("plugin is not installed".to_string());
                return loaded_plugin;
            }
        },
        Err(err) => {
            loaded_plugin.error = Some(err.to_string());
            return loaded_plugin;
        }
    };

    if !plugin_root.as_path().is_dir() {
        loaded_plugin.error = Some("path does not exist or is not a directory".to_string());
        return loaded_plugin;
    }

    let Some(manifest) = load_plugin_manifest(plugin_root.as_path()) else {
        loaded_plugin.error = Some("missing or invalid .praxis-plugin/plugin.json".to_string());
        return loaded_plugin;
    };

    let manifest_paths = &manifest.paths;
    loaded_plugin.manifest_name = manifest
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .map(str::trim)
        .filter(|display_name| !display_name.is_empty())
        .map(str::to_string)
        .or_else(|| Some(manifest.name.clone()));
    loaded_plugin.manifest_description = manifest.description.clone();
    loaded_plugin.llm = manifest.llm.clone();
    loaded_plugin.skill_roots = plugin_skill_roots(plugin_root.as_path(), manifest_paths);
    let resolved_skills = load_plugin_skills(
        plugin_root.as_path(),
        manifest_paths,
        restriction_product,
        skill_config_rules,
    );
    let has_enabled_skills = resolved_skills.has_enabled_skills();
    loaded_plugin.disabled_skill_paths = resolved_skills.disabled_skill_paths;
    loaded_plugin.has_enabled_skills = has_enabled_skills;
    let mut mcp_servers = HashMap::new();
    for mcp_config_path in plugin_mcp_config_paths(plugin_root.as_path(), manifest_paths) {
        let plugin_mcp = load_mcp_servers_from_file(plugin_root.as_path(), &mcp_config_path);
        for (name, config) in plugin_mcp.mcp_servers {
            if mcp_servers.insert(name.clone(), config).is_some() {
                warn!(
                    plugin = %plugin_root.display(),
                    path = %mcp_config_path.display(),
                    server = name,
                    "plugin MCP file overwrote an earlier server definition"
                );
            }
        }
    }
    loaded_plugin.mcp_servers = mcp_servers;
    loaded_plugin.apps = load_plugin_apps(plugin_root.as_path());
    loaded_plugin
}

struct ResolvedPluginSkills {
    skills: Vec<SkillMetadata>,
    disabled_skill_paths: HashSet<PathBuf>,
    had_errors: bool,
}

impl ResolvedPluginSkills {
    fn has_enabled_skills(&self) -> bool {
        // Keep the plugin visible in capability summaries if skill loading was partial.
        self.had_errors
            || self
                .skills
                .iter()
                .any(|skill| !self.disabled_skill_paths.contains(&skill.path_to_skills_md))
    }
}

fn load_plugin_skills(
    plugin_root: &Path,
    manifest_paths: &PluginManifestPaths,
    restriction_product: Option<Product>,
    skill_config_rules: &SkillConfigRules,
) -> ResolvedPluginSkills {
    let outcome = load_skills_from_roots(
        plugin_skill_roots(plugin_root, manifest_paths)
            .into_iter()
            .map(|path| SkillRoot {
                path,
                scope: SkillScope::User,
            }),
    );
    let had_errors = !outcome.errors.is_empty();
    let skills = outcome
        .skills
        .into_iter()
        .filter(|skill| skill.matches_product_restriction_for_product(restriction_product))
        .collect::<Vec<_>>();
    let disabled_skill_paths = resolve_disabled_skill_paths(&skills, skill_config_rules);

    ResolvedPluginSkills {
        skills,
        disabled_skill_paths,
        had_errors,
    }
}

fn plugin_skill_roots(plugin_root: &Path, manifest_paths: &PluginManifestPaths) -> Vec<PathBuf> {
    let mut paths = default_skill_roots(plugin_root);
    if let Some(path) = &manifest_paths.skills {
        paths.push(path.to_path_buf());
    }
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn default_skill_roots(plugin_root: &Path) -> Vec<PathBuf> {
    let skills_dir = plugin_root.join(DEFAULT_SKILLS_DIR_NAME);
    if skills_dir.is_dir() {
        vec![skills_dir]
    } else {
        Vec::new()
    }
}

fn plugin_mcp_config_paths(
    plugin_root: &Path,
    manifest_paths: &PluginManifestPaths,
) -> Vec<AbsolutePathBuf> {
    if let Some(path) = &manifest_paths.mcp_servers {
        return vec![path.clone()];
    }
    default_mcp_config_paths(plugin_root)
}

fn default_mcp_config_paths(plugin_root: &Path) -> Vec<AbsolutePathBuf> {
    let mut paths = Vec::new();
    let default_path = plugin_root.join(DEFAULT_MCP_CONFIG_FILE);
    if default_path.is_file()
        && let Ok(default_path) = AbsolutePathBuf::try_from(default_path)
    {
        paths.push(default_path);
    }
    paths.sort_unstable_by(|left, right| left.as_path().cmp(right.as_path()));
    paths.dedup_by(|left, right| left.as_path() == right.as_path());
    paths
}

pub fn load_plugin_apps(plugin_root: &Path) -> Vec<AppConnectorId> {
    if let Some(manifest) = load_plugin_manifest(plugin_root) {
        return load_apps_from_paths(
            plugin_root,
            plugin_app_config_paths(plugin_root, &manifest.paths),
        );
    }
    load_apps_from_paths(plugin_root, default_app_config_paths(plugin_root))
}

fn plugin_app_config_paths(
    plugin_root: &Path,
    manifest_paths: &PluginManifestPaths,
) -> Vec<AbsolutePathBuf> {
    if let Some(path) = &manifest_paths.apps {
        return vec![path.clone()];
    }
    default_app_config_paths(plugin_root)
}

fn default_app_config_paths(plugin_root: &Path) -> Vec<AbsolutePathBuf> {
    let mut paths = Vec::new();
    let default_path = plugin_root.join(DEFAULT_APP_CONFIG_FILE);
    if default_path.is_file()
        && let Ok(default_path) = AbsolutePathBuf::try_from(default_path)
    {
        paths.push(default_path);
    }
    paths.sort_unstable_by(|left, right| left.as_path().cmp(right.as_path()));
    paths.dedup_by(|left, right| left.as_path() == right.as_path());
    paths
}

fn load_apps_from_paths(
    plugin_root: &Path,
    app_config_paths: Vec<AbsolutePathBuf>,
) -> Vec<AppConnectorId> {
    let mut connector_ids = Vec::new();
    for app_config_path in app_config_paths {
        let Ok(contents) = fs::read_to_string(app_config_path.as_path()) else {
            continue;
        };
        let parsed = match serde_json::from_str::<PluginAppFile>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                warn!(
                    path = %app_config_path.display(),
                    "failed to parse plugin app config: {err}"
                );
                continue;
            }
        };

        let mut apps: Vec<PluginAppConfig> = parsed.apps.into_values().collect();
        apps.sort_unstable_by(|left, right| left.id.cmp(&right.id));

        connector_ids.extend(apps.into_iter().filter_map(|app| {
            if app.id.trim().is_empty() {
                warn!(
                    plugin = %plugin_root.display(),
                    "plugin app config is missing an app id"
                );
                None
            } else {
                Some(AppConnectorId(app.id))
            }
        }));
    }
    connector_ids.dedup();
    connector_ids
}

pub fn plugin_telemetry_metadata_from_root(
    plugin_id: &PluginId,
    plugin_root: &Path,
) -> PluginTelemetryMetadata {
    let Some(manifest) = load_plugin_manifest(plugin_root) else {
        return PluginTelemetryMetadata::from_plugin_id(plugin_id);
    };

    let manifest_paths = &manifest.paths;
    let has_skills = !plugin_skill_roots(plugin_root, manifest_paths).is_empty();
    let mut mcp_server_names = Vec::new();
    for path in plugin_mcp_config_paths(plugin_root, manifest_paths) {
        mcp_server_names.extend(
            load_mcp_servers_from_file(plugin_root, &path)
                .mcp_servers
                .into_keys(),
        );
    }
    mcp_server_names.sort_unstable();
    mcp_server_names.dedup();

    PluginTelemetryMetadata {
        plugin_id: plugin_id.clone(),
        capability_summary: Some(PluginCapabilitySummary {
            config_name: plugin_id.as_key(),
            display_name: plugin_id.plugin_name.clone(),
            description: None,
            has_skills,
            has_llm: manifest.llm.is_some(),
            mcp_server_names,
            app_connector_ids: load_plugin_apps(plugin_root),
        }),
    }
}

pub fn load_plugin_mcp_servers(plugin_root: &Path) -> HashMap<String, McpServerConfig> {
    let Some(manifest) = load_plugin_manifest(plugin_root) else {
        return HashMap::new();
    };

    let mut mcp_servers = HashMap::new();
    for mcp_config_path in plugin_mcp_config_paths(plugin_root, &manifest.paths) {
        let plugin_mcp = load_mcp_servers_from_file(plugin_root, &mcp_config_path);
        for (name, config) in plugin_mcp.mcp_servers {
            mcp_servers.entry(name).or_insert(config);
        }
    }

    mcp_servers
}

pub fn plugin_activation_delta_from_root(
    plugin_id: &PluginId,
    plugin_root: &Path,
) -> PluginActivationDelta<McpServerConfig> {
    let skill_roots = load_plugin_manifest(plugin_root)
        .map(|manifest| plugin_skill_roots(plugin_root, &manifest.paths))
        .unwrap_or_default();
    let mcp_servers = load_plugin_mcp_servers(plugin_root)
        .into_iter()
        .collect::<Vec<_>>();
    let app_connector_ids = load_plugin_apps(plugin_root);

    PluginActivationDelta {
        plugin_id: Some(plugin_id.clone()),
        installed_path: AbsolutePathBuf::try_from(plugin_root.to_path_buf()).ok(),
        changes: PluginCapabilityChanges {
            skills_changed: !skill_roots.is_empty(),
            mcp_servers_changed: !mcp_servers.is_empty(),
            apps_changed: !app_connector_ids.is_empty(),
            skill_roots,
            mcp_servers,
            app_connector_ids,
        },
        diagnostics: Vec::new(),
    }
}

pub fn installed_plugin_telemetry_metadata(
    praxis_home: &Path,
    plugin_id: &PluginId,
) -> PluginTelemetryMetadata {
    let store = PluginStore::new(praxis_home.to_path_buf());
    let Some(plugin_root) = store.active_plugin_root(plugin_id) else {
        return PluginTelemetryMetadata::from_plugin_id(plugin_id);
    };

    plugin_telemetry_metadata_from_root(plugin_id, plugin_root.as_path())
}

fn load_mcp_servers_from_file(
    plugin_root: &Path,
    mcp_config_path: &AbsolutePathBuf,
) -> PluginMcpDiscovery {
    let Ok(contents) = fs::read_to_string(mcp_config_path.as_path()) else {
        return PluginMcpDiscovery::default();
    };
    let parsed = match serde_json::from_str::<PluginMcpFile>(&contents) {
        Ok(parsed) => parsed,
        Err(err) => {
            warn!(
                path = %mcp_config_path.display(),
                "failed to parse plugin MCP config: {err}"
            );
            return PluginMcpDiscovery::default();
        }
    };
    normalize_plugin_mcp_servers(
        plugin_root,
        parsed.mcp_servers,
        mcp_config_path.to_string_lossy().as_ref(),
    )
}

fn normalize_plugin_mcp_servers(
    plugin_root: &Path,
    plugin_mcp_servers: HashMap<String, JsonValue>,
    source: &str,
) -> PluginMcpDiscovery {
    let mut mcp_servers = HashMap::new();

    for (name, config_value) in plugin_mcp_servers {
        let normalized = normalize_plugin_mcp_server_value(plugin_root, config_value);
        match serde_json::from_value::<McpServerConfig>(JsonValue::Object(normalized)) {
            Ok(config) => {
                mcp_servers.insert(name, config);
            }
            Err(err) => {
                warn!(
                    plugin = %plugin_root.display(),
                    server = name,
                    "failed to parse plugin MCP server from {source}: {err}"
                );
            }
        }
    }

    PluginMcpDiscovery { mcp_servers }
}

fn normalize_plugin_mcp_server_value(
    plugin_root: &Path,
    value: JsonValue,
) -> JsonMap<String, JsonValue> {
    let mut object = match value {
        JsonValue::Object(object) => object,
        _ => return JsonMap::new(),
    };

    if let Some(JsonValue::String(transport_type)) = object.remove("type") {
        match transport_type.as_str() {
            "http" | "streamable_http" | "streamable-http" => {}
            "stdio" => {}
            other => {
                warn!(
                    plugin = %plugin_root.display(),
                    transport = other,
                    "plugin MCP server uses an unknown transport type"
                );
            }
        }
    }

    if let Some(JsonValue::Object(oauth)) = object.remove("oauth")
        && oauth.contains_key("callbackPort")
    {
        warn!(
            plugin = %plugin_root.display(),
            "plugin MCP server OAuth callbackPort is ignored; Praxis uses global MCP OAuth callback settings"
        );
    }

    if let Some(JsonValue::String(cwd)) = object.get("cwd")
        && !Path::new(cwd).is_absolute()
    {
        object.insert(
            "cwd".to_string(),
            JsonValue::String(plugin_root.join(cwd).display().to_string()),
        );
    }

    object
}

#[derive(Debug, Default)]
struct PluginMcpDiscovery {
    mcp_servers: HashMap<String, McpServerConfig>,
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

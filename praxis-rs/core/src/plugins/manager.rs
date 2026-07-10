use super::LoadedPlugin;
use super::PluginLoadOutcome;
use super::curated::curated_plugins_marketplace_path;
use super::curated::curated_plugins_repo_path;
use super::curated::is_openai_curated_marketplace;
use super::curated::openai_curated_marketplace_display_name;
use super::curated::openai_curated_marketplace_interface;
use super::curated::unique_curated_marketplace_plugins;
use super::manifest::PluginManifestInterface;
use super::manifest::PluginManifestLlm;
use super::manifest::PluginManifestPaths;
use super::manifest::load_plugin_manifest;
use super::marketplace::MarketplaceError;
use super::marketplace::MarketplaceInterface;
use super::marketplace::MarketplaceListError;
use super::marketplace::MarketplacePluginAuthPolicy;
use super::marketplace::MarketplacePluginPolicy;
use super::marketplace::MarketplacePluginSource;
use super::marketplace::ResolvedMarketplacePlugin;
use super::marketplace::list_marketplaces;
use super::marketplace::load_marketplace;
use super::marketplace::marketplace_manifest_path;
use super::marketplace::resolve_marketplace_plugin;
use super::remote::RemotePluginFetchError;
use super::remote::RemotePluginMutationError;
use super::remote::enable_remote_plugin;
use super::remote::fetch_remote_featured_plugin_ids;
use super::remote::fetch_remote_plugin_status;
use super::remote::uninstall_remote_plugin;
use super::startup_sync::read_curated_plugins_sha;
use super::startup_sync::spawn_logged_startup_task;
use super::startup_sync::start_startup_remote_plugin_sync_once;
use super::startup_sync::sync_curated_plugins_repo;
use super::store::PluginInstallResult as StorePluginInstallResult;
use super::store::PluginStore;
use super::store::PluginStoreError;
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
use praxis_login::OpenAiAccountAuth;
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
    auth: Option<&OpenAiAccountAuth>,
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
            commands: value
                .interface
                .as_ref()
                .map(|interface| {
                    interface
                        .commands
                        .iter()
                        .map(|command| praxis_plugin::PluginCommandSummary {
                            name: command.name.clone(),
                            description: command.description.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
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

mod errors;
mod load_plugin;
mod manager_impl;
mod marketplace_provider;

pub use self::errors::{PluginInstallError, PluginSetEnabledError, PluginUninstallError};
pub use self::load_plugin::{
    installed_plugin_telemetry_metadata, load_plugin_apps, load_plugin_mcp_servers,
    plugin_activation_delta_from_root, plugin_telemetry_metadata_from_root,
};

pub struct PluginsManager {
    praxis_home: PathBuf,
    store: PluginStore,
    featured_plugin_ids_cache: RwLock<Option<CachedFeaturedPluginIds>>,
    cached_enabled_outcome: RwLock<Option<PluginLoadOutcome>>,
    remote_sync_lock: Mutex<()>,
    restriction_product: Option<Product>,
    analytics_events_client: RwLock<Option<AnalyticsEventsClient>>,
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

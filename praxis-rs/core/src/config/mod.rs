use crate::config_loader::ConfigLayerStack;
use crate::config_loader::ConfigRequirements;
#[cfg(test)]
use crate::config_loader::McpServerIdentity;
#[cfg(test)]
use crate::config_loader::McpServerRequirement;
use crate::config_loader::Sourced;
use crate::memories::memory_root;
use crate::model_provider_info::LEGACY_OLLAMA_CHAT_PROVIDER_ID;
use crate::model_provider_info::OLLAMA_CHAT_PROVIDER_REMOVED_ERROR;
use crate::model_provider_info::built_in_model_providers;
use crate::path_utils::normalize_for_native_workdir;
use crate::project_doc::DEFAULT_PROJECT_DOC_FILENAME;
use crate::project_doc::LOCAL_PROJECT_DOC_FILENAME;
use crate::unified_exec::DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS;
use crate::unified_exec::MIN_EMPTY_YIELD_TIME_MS;
use crate::windows_sandbox::WindowsSandboxLevelExt;
use crate::windows_sandbox::resolve_windows_sandbox_mode;
use crate::windows_sandbox::resolve_windows_sandbox_private_desktop;
use praxis_config::types::ApprovalsReviewer;
use praxis_config::types::DEFAULT_OTEL_ENVIRONMENT;
#[cfg(test)]
use praxis_config::types::McpServerDisabledReason;
#[cfg(test)]
use praxis_config::types::McpServerTransportConfig;
use praxis_config::types::OtelConfig;
use praxis_config::types::OtelConfigToml;
use praxis_config::types::OtelExporterKind;
use praxis_config::types::UriBasedFileOpener;
use praxis_config::types::WindowsSandboxModeToml;
use praxis_features::Feature;
use praxis_features::FeatureConfigSource;
use praxis_features::FeatureOverrides;
use praxis_features::Features;
use praxis_mcp::mcp::McpConfig;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_absolute_path::AbsolutePathBuf;
use std::path::Path;
use std::path::PathBuf;

use crate::config::permissions::compile_permission_profile;
use crate::config::permissions::get_readable_roots_required_for_praxis_runtime;
use crate::config::permissions::network_proxy_config_from_profile_network;
use crate::config::profile::ConfigProfile;
use praxis_network_proxy::NetworkProxyConfig;
use toml::Value as TomlValue;

pub(crate) mod agent_roles;
mod builder;
mod config_toml;
pub mod edit;
mod home_paths;
mod instructions;
mod load_helpers;
mod managed_features;
mod model_catalog_file;
mod model_provider_config;
mod network_proxy_spec;
mod oss_provider;
mod overrides;
mod permission_syntax;
mod permissions;
pub mod profile;
mod project_trust;
mod requirements_enforcement;
mod runtime_config;
mod runtime_types;
pub mod sandbox_projection;
pub mod schema;
pub mod service;
pub mod service_types;
mod web_search;
use self::instructions::guardian_developer_instructions_from_requirements;
use self::permission_syntax::PermissionConfigSyntax;
use self::permission_syntax::resolve_permission_config_syntax;
use self::requirements_enforcement::apply_requirement_constrained_value;
use self::requirements_enforcement::constrain_mcp_servers;
pub use home_paths::current_praxis_home_namespace;
pub use home_paths::default_external_codex_home;
pub use home_paths::default_praxis_home_for_namespace;
pub use home_paths::find_praxis_home;
pub use home_paths::log_dir;
pub(crate) use load_helpers::deserialize_config_toml_with_base;
pub use load_helpers::load_config_as_toml_with_cli_overrides;
pub use load_helpers::load_global_mcp_servers;
pub use managed_features::ManagedFeatures;
use model_catalog_file::load_model_catalog;
use model_provider_config::normalize_provider_for_selected_model;
use model_provider_config::validate_model_providers;
pub use network_proxy_spec::NetworkProxySpec;
pub use network_proxy_spec::StartedNetworkProxy;
pub use oss_provider::resolve_oss_provider;
pub use oss_provider::set_default_oss_provider;
pub use overrides::ConfigOverrides;
pub use permissions::FilesystemPermissionToml;
pub use permissions::FilesystemPermissionsToml;
pub use permissions::NetworkDomainPermissionToml;
pub use permissions::NetworkDomainPermissionsToml;
pub use permissions::NetworkToml;
pub use permissions::NetworkUnixSocketPermissionToml;
pub use permissions::NetworkUnixSocketPermissionsToml;
pub use permissions::PermissionProfileToml;
pub use permissions::PermissionsToml;
pub(crate) use permissions::overlay_network_domain_permissions;
pub(crate) use permissions::resolve_permission_profile;
pub use praxis_config::Constrained;
pub use praxis_config::ConstraintError;
pub use praxis_config::ConstraintResult;
pub use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_protocol::protocol::Op;
use praxis_protocol::user_input::UserInput;
pub use praxis_sandboxing::system_bwrap_warning;
pub use praxis_utils_home_dir::PraxisHomeNamespace;
pub use project_trust::set_project_trust_level;
pub(crate) use project_trust::set_project_trust_level_inner;
pub use runtime_config::Config;
pub use runtime_config::Permissions;
pub use runtime_types::AgentRoleConfig;
pub use runtime_types::AgentRoleToml;
pub use runtime_types::AgentsToml;
pub use runtime_types::GhostSnapshotToml;
pub use runtime_types::LocalModelHostConfig;
pub use runtime_types::LocalModelHostKind;
pub use runtime_types::LocalModelsConfig;
pub use runtime_types::ProjectConfig;
pub use runtime_types::RealtimeAudioConfig;
pub use runtime_types::RealtimeAudioToml;
pub use runtime_types::RealtimeConfig;
pub use runtime_types::RealtimeToml;
pub use runtime_types::RealtimeWsMode;
pub use runtime_types::RealtimeWsVersion;
pub use runtime_types::ToolsToml;
pub use runtime_types::TranscriptionConfig;
pub use runtime_types::TranscriptionProviderConfig;
pub use runtime_types::TranscriptionProviderKind;
pub use runtime_types::TranscriptionSubmitMode;
use runtime_types::resolve_tool_suggest_config;
use serde_json::Value as JsonValue;
pub use service::ConfigService;
pub use service::ConfigServiceError;
pub use service_types::AppConfig as ServiceAppConfig;
pub use service_types::AppToolApproval as ServiceAppToolApproval;
pub use service_types::AppsConfig as ServiceAppsConfig;
pub use service_types::ConfigBatchWriteParams;
pub use service_types::ConfigReadParams;
pub use service_types::ConfigReadResponse;
pub use service_types::ConfigValueWriteParams;
pub use service_types::ConfigView;
pub use service_types::ConfigWriteEdit;
pub use service_types::ConfigWriteErrorCode;
pub use service_types::ConfigWriteResponse;
pub use service_types::MergeStrategy;
pub use service_types::OverriddenMetadata;
pub use service_types::Profile as ServiceProfile;
pub use service_types::SandboxSettings;
pub use service_types::Tools;
pub use service_types::UserSavedConfig;
pub use service_types::WriteStatus;
use web_search::resolve_web_search_config;
use web_search::resolve_web_search_mode;
pub(crate) use web_search::resolve_web_search_mode_for_turn;

pub use builder::ConfigBuilder;
pub use config_toml::ConfigToml;
pub use praxis_git_utils::GhostSnapshotConfig;

/// Maximum number of bytes of the documentation that will be embedded. Larger
/// files are *silently truncated* to this size so we do not take up too much of
/// the context window.
pub(crate) const PROJECT_DOC_MAX_BYTES: usize = 32 * 1024; // 32 KiB
pub(crate) const DEFAULT_AGENT_MAX_THREADS: Option<usize> = Some(3);
pub(crate) const DEFAULT_AGENT_MAX_DEPTH: i32 = 1;
pub(crate) const DEFAULT_AGENT_JOB_MAX_RUNTIME_SECONDS: Option<u64> = None;

pub const CONFIG_TOML_FILE: &str = "config.toml";

fn resolve_sqlite_home_env(resolved_cwd: &Path) -> Option<PathBuf> {
    let raw = std::env::var(praxis_state::SQLITE_HOME_ENV).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(resolved_cwd.join(path))
    }
}

#[cfg(test)]
pub(crate) fn test_config() -> Config {
    let praxis_home = tempfile::tempdir().expect("create temp dir");
    Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides::default(),
        praxis_home.path().to_path_buf(),
    )
    .expect("load default test config")
}

impl Config {
    pub fn user_turn_op(
        &self,
        items: Vec<UserInput>,
        final_output_json_schema: Option<JsonValue>,
    ) -> Op {
        Op::UserTurn {
            items,
            cwd: self.cwd.to_path_buf(),
            approval_policy: self.permissions.approval_policy.value(),
            approvals_reviewer: Some(self.approvals_reviewer.clone()),
            sandbox_policy: self.permissions.sandbox_policy.get().clone(),
            model: self
                .model
                .clone()
                .unwrap_or_else(|| "praxis-auto".to_string()),
            model_provider: Some(self.model_provider_id.clone()),
            effort: self.model_reasoning_effort.clone(),
            summary: self.model_reasoning_summary.clone(),
            service_tier: Some(self.service_tier.clone()),
            final_output_json_schema,
            collaboration_mode: None,
            personality: self.personality.clone(),
        }
    }

    pub fn to_mcp_config(&self, plugins_manager: &crate::plugins::PluginsManager) -> McpConfig {
        let loaded_plugins = plugins_manager.plugins_for_config(self);
        let mut configured_mcp_servers = self.mcp_servers.get().clone();
        for (name, plugin_server) in loaded_plugins.effective_mcp_servers() {
            configured_mcp_servers.entry(name).or_insert(plugin_server);
        }

        McpConfig {
            chatgpt_base_url: self.chatgpt_base_url.clone(),
            praxis_home: self.praxis_home.clone(),
            mcp_oauth_credentials_store_mode: self.mcp_oauth_credentials_store_mode,
            mcp_oauth_callback_port: self.mcp_oauth_callback_port,
            mcp_oauth_callback_url: self.mcp_oauth_callback_url.clone(),
            skill_mcp_dependency_install_enabled: self
                .features
                .enabled(Feature::SkillMcpDependencyInstall),
            approval_policy: self.permissions.approval_policy.clone(),
            praxis_linux_sandbox_exe: self.praxis_linux_sandbox_exe.clone(),
            use_legacy_landlock: self.features.use_legacy_landlock(),
            apps_enabled: self.features.enabled(Feature::Apps),
            configured_mcp_servers,
            plugin_capability_summaries: loaded_plugins.capability_summaries().to_vec(),
        }
    }

    /// This is the preferred way to create an instance of [Config].
    pub async fn load_with_cli_overrides(
        cli_overrides: Vec<(String, TomlValue)>,
    ) -> std::io::Result<Self> {
        ConfigBuilder::default()
            .cli_overrides(cli_overrides)
            .build()
            .await
    }

    /// Load a default configuration when user config files are invalid.
    pub fn load_default_with_cli_overrides(
        cli_overrides: Vec<(String, TomlValue)>,
    ) -> std::io::Result<Self> {
        let praxis_home = find_praxis_home()?;
        Self::load_default_with_cli_overrides_for_praxis_home(praxis_home, cli_overrides)
    }

    /// Load a default configuration for a specific Praxis home without reading
    /// user, project, or system config layers.
    pub fn load_default_with_cli_overrides_for_praxis_home(
        praxis_home: PathBuf,
        cli_overrides: Vec<(String, TomlValue)>,
    ) -> std::io::Result<Self> {
        let mut merged = toml::Value::try_from(ConfigToml::default()).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to serialize default config: {e}"),
            )
        })?;
        let cli_layer = crate::config_loader::build_cli_overrides_layer(&cli_overrides);
        crate::config_loader::merge_toml_values(&mut merged, &cli_layer);
        let config_toml = deserialize_config_toml_with_base(merged, &praxis_home)?;
        Self::load_config_with_layer_stack(
            config_toml,
            ConfigOverrides::default(),
            praxis_home,
            ConfigLayerStack::default(),
        )
    }

    /// This is a secondary way of creating [Config], which is appropriate when
    /// the harness is meant to be used with a specific configuration that
    /// ignores user settings. For example, the `praxis exec` subcommand is
    /// designed to use [AskForApproval::Never] exclusively.
    ///
    /// Further, [ConfigOverrides] contains some options that are not supported
    /// in [ConfigToml], such as `cwd`, `praxis_self_exe`, `praxis_linux_sandbox_exe`, and
    /// `main_execve_wrapper_exe`.
    pub async fn load_with_cli_overrides_and_harness_overrides(
        cli_overrides: Vec<(String, TomlValue)>,
        harness_overrides: ConfigOverrides,
    ) -> std::io::Result<Self> {
        ConfigBuilder::default()
            .cli_overrides(cli_overrides)
            .harness_overrides(harness_overrides)
            .build()
            .await
    }
}

impl Config {
    #[cfg(test)]
    fn load_from_base_config_with_overrides(
        cfg: ConfigToml,
        overrides: ConfigOverrides,
        praxis_home: PathBuf,
    ) -> std::io::Result<Self> {
        // Note this ignores requirements.toml enforcement for tests.
        let config_layer_stack = ConfigLayerStack::default();
        Self::load_config_with_layer_stack(cfg, overrides, praxis_home, config_layer_stack)
    }

    pub(crate) fn load_config_with_layer_stack(
        cfg: ConfigToml,
        overrides: ConfigOverrides,
        praxis_home: PathBuf,
        config_layer_stack: ConfigLayerStack,
    ) -> std::io::Result<Self> {
        validate_model_providers(&cfg.model_providers)
            .map_err(|message| std::io::Error::new(std::io::ErrorKind::InvalidInput, message))?;
        // Ensure that every field of ConfigRequirements is applied to the final
        // Config.
        let ConfigRequirements {
            approval_policy: mut constrained_approval_policy,
            sandbox_policy: mut constrained_sandbox_policy,
            web_search_mode: mut constrained_web_search_mode,
            feature_requirements,
            mcp_servers,
            exec_policy: _,
            enforce_residency,
            network: network_requirements,
        } = config_layer_stack.requirements().clone();

        let user_instructions = Self::load_instructions(Some(&praxis_home));
        let mut startup_warnings = Vec::new();

        // Destructure ConfigOverrides fully to ensure all overrides are applied.
        let ConfigOverrides {
            model,
            review_model: override_review_model,
            cwd,
            approval_policy: approval_policy_override,
            approvals_reviewer: approvals_reviewer_override,
            sandbox_mode,
            model_provider,
            service_tier: service_tier_override,
            config_profile: config_profile_key,
            praxis_self_exe,
            praxis_linux_sandbox_exe,
            main_execve_wrapper_exe,
            zsh_path: zsh_path_override,
            base_instructions,
            developer_instructions,
            personality,
            compact_prompt,
            include_apply_patch_tool: include_apply_patch_tool_override,
            show_raw_agent_reasoning,
            tools_web_search_request: override_tools_web_search_request,
            ephemeral,
            additional_writable_roots,
        } = overrides;

        let active_profile_name = config_profile_key
            .as_ref()
            .or(cfg.profile.as_ref())
            .cloned();
        let config_profile = match active_profile_name.as_ref() {
            Some(key) => cfg
                .profiles
                .get(key)
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("config profile `{key}` not found"),
                    )
                })?
                .clone(),
            None => ConfigProfile::default(),
        };
        let tool_suggest = resolve_tool_suggest_config(&cfg);
        let feature_overrides = FeatureOverrides {
            include_apply_patch_tool: include_apply_patch_tool_override,
            web_search_request: override_tools_web_search_request,
        };

        let configured_features = Features::from_sources(
            FeatureConfigSource {
                features: cfg.features.as_ref(),
            },
            FeatureConfigSource {
                features: config_profile.features.as_ref(),
            },
            feature_overrides,
        );
        let features = ManagedFeatures::from_configured(configured_features, feature_requirements)?;
        let windows_sandbox_mode = resolve_windows_sandbox_mode(&cfg, &config_profile);
        let windows_sandbox_private_desktop =
            resolve_windows_sandbox_private_desktop(&cfg, &config_profile);
        let resolved_cwd = AbsolutePathBuf::try_from(normalize_for_native_workdir({
            use std::env;

            match cwd {
                None => {
                    tracing::info!("cwd not set, using current dir");
                    env::current_dir()?
                }
                Some(p) if p.is_absolute() => p,
                Some(p) => {
                    // Resolve relative path against the current working directory.
                    tracing::info!("cwd is relative, resolving against current dir");
                    let mut current = env::current_dir()?;
                    current.push(p);
                    current
                }
            }
        }))?;
        let mut additional_writable_roots: Vec<AbsolutePathBuf> = additional_writable_roots
            .into_iter()
            .map(|path| AbsolutePathBuf::resolve_path_against_base(path, resolved_cwd.as_path()))
            .collect::<Result<Vec<_>, _>>()?;
        let active_project = cfg
            .get_active_project(resolved_cwd.as_path())
            .unwrap_or(ProjectConfig { trust_level: None });
        let permission_config_syntax = resolve_permission_config_syntax(
            &config_layer_stack,
            &cfg,
            sandbox_mode,
            config_profile.sandbox_mode,
        );
        let has_permission_profiles = cfg
            .permissions
            .as_ref()
            .is_some_and(|profiles| !profiles.is_empty());
        if has_permission_profiles
            && !matches!(
                permission_config_syntax,
                Some(PermissionConfigSyntax::Legacy)
            )
            && cfg.default_permissions.is_none()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "config defines `[permissions]` profiles but does not set `default_permissions`",
            ));
        }

        let windows_sandbox_level = match windows_sandbox_mode {
            Some(WindowsSandboxModeToml::Elevated) => WindowsSandboxLevel::Elevated,
            Some(WindowsSandboxModeToml::Unelevated) => WindowsSandboxLevel::RestrictedToken,
            None => WindowsSandboxLevel::from_features(&features),
        };
        let memories_root = memory_root(&praxis_home);
        std::fs::create_dir_all(&memories_root)?;
        let memories_root = AbsolutePathBuf::from_absolute_path(&memories_root)?;
        if !additional_writable_roots
            .iter()
            .any(|existing| existing == &memories_root)
        {
            additional_writable_roots.push(memories_root);
        }

        let profiles_are_active = matches!(
            permission_config_syntax,
            Some(PermissionConfigSyntax::Profiles)
        ) || (permission_config_syntax.is_none()
            && has_permission_profiles);
        let (
            configured_network_proxy_config,
            sandbox_policy,
            file_system_sandbox_policy,
            network_sandbox_policy,
        ) = if profiles_are_active {
            let permissions = cfg.permissions.as_ref().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "default_permissions requires a `[permissions]` table",
                )
            })?;
            let default_permissions = cfg.default_permissions.as_deref().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "default_permissions requires a named permissions profile",
                )
            })?;
            let profile = resolve_permission_profile(permissions, default_permissions)?;
            let configured_network_proxy_config =
                network_proxy_config_from_profile_network(profile.network.as_ref());
            let (mut file_system_sandbox_policy, network_sandbox_policy) =
                compile_permission_profile(
                    permissions,
                    default_permissions,
                    &mut startup_warnings,
                )?;
            let mut sandbox_policy = sandbox_projection::sandbox_policy_from_split(
                &file_system_sandbox_policy,
                network_sandbox_policy,
                resolved_cwd.as_path(),
            )?;
            if matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }) {
                file_system_sandbox_policy = file_system_sandbox_policy
                    .with_additional_writable_roots(
                        resolved_cwd.as_path(),
                        &additional_writable_roots,
                    );
                sandbox_policy = sandbox_projection::sandbox_policy_from_split(
                    &file_system_sandbox_policy,
                    network_sandbox_policy,
                    resolved_cwd.as_path(),
                )?;
            }
            (
                configured_network_proxy_config,
                sandbox_policy,
                file_system_sandbox_policy,
                network_sandbox_policy,
            )
        } else {
            let configured_network_proxy_config = NetworkProxyConfig::default();
            let mut sandbox_policy = cfg.derive_sandbox_policy(
                sandbox_mode,
                config_profile.sandbox_mode,
                windows_sandbox_level,
                resolved_cwd.as_path(),
                Some(&constrained_sandbox_policy),
            );
            if let SandboxPolicy::WorkspaceWrite { writable_roots, .. } = &mut sandbox_policy {
                for path in &additional_writable_roots {
                    if !writable_roots.iter().any(|existing| existing == path) {
                        writable_roots.push(path.clone());
                    }
                }
            }
            let (file_system_sandbox_policy, network_sandbox_policy) =
                sandbox_projection::split_sandbox_policy(&sandbox_policy, resolved_cwd.as_path());
            (
                configured_network_proxy_config,
                sandbox_policy,
                file_system_sandbox_policy,
                network_sandbox_policy,
            )
        };
        let approval_policy_was_explicit = approval_policy_override.is_some()
            || config_profile.approval_policy.is_some()
            || cfg.approval_policy.is_some();
        let mut approval_policy = approval_policy_override
            .or(config_profile.approval_policy)
            .or(cfg.approval_policy)
            .unwrap_or_else(|| {
                if active_project.is_trusted() {
                    AskForApproval::OnRequest
                } else if active_project.is_untrusted() {
                    AskForApproval::UnlessTrusted
                } else {
                    AskForApproval::default()
                }
            });
        if !approval_policy_was_explicit
            && let Err(err) = constrained_approval_policy.can_set(&approval_policy)
        {
            tracing::warn!(
                error = %err,
                "default approval policy is disallowed by requirements; falling back to required default"
            );
            approval_policy = constrained_approval_policy.value();
        }
        let approvals_reviewer = approvals_reviewer_override
            .or(config_profile.approvals_reviewer)
            .or(cfg.approvals_reviewer)
            .unwrap_or(ApprovalsReviewer::User);
        let web_search_mode = resolve_web_search_mode(&cfg, &config_profile, &features)
            .unwrap_or(WebSearchMode::Cached);
        let web_search_config = resolve_web_search_config(&cfg, &config_profile);

        let agent_roles =
            agent_roles::load_agent_roles(&cfg, &config_layer_stack, &mut startup_warnings)?;

        let openai_base_url = cfg
            .openai_base_url
            .clone()
            .filter(|value| !value.is_empty());

        let mut model_providers = built_in_model_providers(openai_base_url);
        // Merge user-defined providers into the built-in list.
        for (key, provider) in cfg.model_providers.into_iter() {
            model_providers.entry(key).or_insert(provider);
        }

        let explicit_model_provider =
            model_provider.is_some() || config_profile.model_provider.is_some();
        let model_provider_id = model_provider
            .or(config_profile.model_provider)
            .or(cfg.model_provider)
            .unwrap_or_else(|| "openai".to_string());
        let model_provider = model_providers
            .get(&model_provider_id)
            .ok_or_else(|| {
                let message = if model_provider_id == LEGACY_OLLAMA_CHAT_PROVIDER_ID {
                    OLLAMA_CHAT_PROVIDER_REMOVED_ERROR.to_string()
                } else {
                    format!("Model provider `{model_provider_id}` not found")
                };
                std::io::Error::new(std::io::ErrorKind::NotFound, message)
            })?
            .clone();

        let shell_environment_policy = cfg.shell_environment_policy.into();
        let allow_login_shell = cfg.allow_login_shell.unwrap_or(true);

        let history = cfg.history.unwrap_or_default();

        let agent_max_threads = cfg
            .agents
            .as_ref()
            .and_then(|agents| agents.max_threads)
            .or(DEFAULT_AGENT_MAX_THREADS);
        if agent_max_threads == Some(0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "agents.max_threads must be at least 1",
            ));
        }
        let agent_max_depth = cfg
            .agents
            .as_ref()
            .and_then(|agents| agents.max_depth)
            .unwrap_or(DEFAULT_AGENT_MAX_DEPTH);
        if agent_max_depth < 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "agents.max_depth must be at least 1",
            ));
        }
        let agent_job_max_runtime_seconds = cfg
            .agents
            .as_ref()
            .and_then(|agents| agents.job_max_runtime_seconds)
            .or(DEFAULT_AGENT_JOB_MAX_RUNTIME_SECONDS);
        if agent_job_max_runtime_seconds == Some(0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "agents.job_max_runtime_seconds must be at least 1",
            ));
        }
        if let Some(max_runtime_seconds) = agent_job_max_runtime_seconds
            && max_runtime_seconds > i64::MAX as u64
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "agents.job_max_runtime_seconds must fit within a 64-bit signed integer",
            ));
        }
        let background_terminal_max_timeout = cfg
            .background_terminal_max_timeout
            .unwrap_or(DEFAULT_MAX_BACKGROUND_TERMINAL_TIMEOUT_MS)
            .max(MIN_EMPTY_YIELD_TIME_MS);

        let ghost_snapshot = {
            let mut config = GhostSnapshotConfig::default();
            if let Some(ghost_snapshot) = cfg.ghost_snapshot.as_ref()
                && let Some(ignore_over_bytes) = ghost_snapshot.ignore_large_untracked_files
            {
                config.ignore_large_untracked_files = if ignore_over_bytes > 0 {
                    Some(ignore_over_bytes)
                } else {
                    None
                };
            }
            if let Some(ghost_snapshot) = cfg.ghost_snapshot.as_ref()
                && let Some(threshold) = ghost_snapshot.ignore_large_untracked_dirs
            {
                config.ignore_large_untracked_dirs =
                    if threshold > 0 { Some(threshold) } else { None };
            }
            if let Some(ghost_snapshot) = cfg.ghost_snapshot.as_ref()
                && let Some(disable_warnings) = ghost_snapshot.disable_warnings
            {
                config.disable_warnings = disable_warnings;
            }
            config
        };

        let include_apply_patch_tool_flag = features.enabled(Feature::ApplyPatchFreeform);
        let use_experimental_unified_exec_tool = features.enabled(Feature::UnifiedExec);

        let forced_chatgpt_workspace_id =
            cfg.forced_chatgpt_workspace_id.as_ref().and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

        let forced_login_method = cfg.forced_login_method;

        let model = model.or(config_profile.model).or(cfg.model);
        let (model_provider_id, model_provider) = normalize_provider_for_selected_model(
            model_provider_id,
            model_provider,
            model.as_deref(),
            explicit_model_provider,
            &model_providers,
            &mut startup_warnings,
        );
        let service_tier = service_tier_override
            .unwrap_or_else(|| config_profile.service_tier.or(cfg.service_tier));
        let service_tier = match service_tier {
            Some(ServiceTier::Fast) if features.enabled(Feature::FastMode) => {
                Some(ServiceTier::Fast)
            }
            Some(ServiceTier::Flex) => Some(ServiceTier::Flex),
            _ => None,
        };

        let compact_prompt = compact_prompt.or(cfg.compact_prompt).and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        let commit_attribution = cfg.commit_attribution;

        // Load base instructions override from a file if specified. If the
        // path is relative, resolve it against the effective cwd so the
        // behaviour matches other path-like config values.
        let model_instructions_path = config_profile
            .model_instructions_file
            .as_ref()
            .or(cfg.model_instructions_file.as_ref());
        let file_base_instructions =
            Self::try_read_non_empty_file(model_instructions_path, "model instructions file")?;
        let base_instructions = base_instructions.or(file_base_instructions);
        let developer_instructions = developer_instructions.or(cfg.developer_instructions);
        let guardian_developer_instructions = guardian_developer_instructions_from_requirements(
            config_layer_stack.requirements_toml(),
        );
        let personality = personality
            .or(config_profile.personality)
            .or(cfg.personality)
            .or_else(|| {
                features
                    .enabled(Feature::Personality)
                    .then_some(Personality::Pragmatic)
            });

        let experimental_compact_prompt_path = config_profile
            .experimental_compact_prompt_file
            .as_ref()
            .or(cfg.experimental_compact_prompt_file.as_ref());
        let file_compact_prompt = Self::try_read_non_empty_file(
            experimental_compact_prompt_path,
            "experimental compact prompt file",
        )?;
        let compact_prompt = compact_prompt.or(file_compact_prompt);
        let zsh_path = zsh_path_override
            .or(config_profile.zsh_path.map(Into::into))
            .or(cfg.zsh_path.map(Into::into));

        let review_model = override_review_model.or(cfg.review_model);

        let check_for_update_on_startup = cfg.check_for_update_on_startup.unwrap_or(true);
        let model_catalog = load_model_catalog(
            config_profile
                .model_catalog_json
                .clone()
                .or(cfg.model_catalog_json.clone()),
        )?;

        let log_dir = cfg
            .log_dir
            .as_ref()
            .map(AbsolutePathBuf::to_path_buf)
            .unwrap_or_else(|| {
                let mut p = praxis_home.clone();
                p.push("log");
                p
            });
        let sqlite_home = cfg
            .sqlite_home
            .as_ref()
            .map(AbsolutePathBuf::to_path_buf)
            .or_else(|| resolve_sqlite_home_env(&resolved_cwd))
            .unwrap_or_else(|| praxis_home.to_path_buf());
        let original_sandbox_policy = sandbox_policy.clone();

        apply_requirement_constrained_value(
            "approval_policy",
            approval_policy,
            &mut constrained_approval_policy,
            &mut startup_warnings,
        )?;
        apply_requirement_constrained_value(
            "sandbox_mode",
            sandbox_policy,
            &mut constrained_sandbox_policy,
            &mut startup_warnings,
        )?;
        apply_requirement_constrained_value(
            "web_search_mode",
            web_search_mode,
            &mut constrained_web_search_mode,
            &mut startup_warnings,
        )?;

        let mcp_servers = constrain_mcp_servers(cfg.mcp_servers.clone(), mcp_servers.as_ref())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{e}")))?;

        let (network_requirements, network_requirements_source) = match network_requirements {
            Some(Sourced { value, source }) => (Some(value), Some(source)),
            None => (None, None),
        };
        let has_network_requirements = network_requirements.is_some();
        let network = NetworkProxySpec::from_config_and_constraints(
            configured_network_proxy_config,
            network_requirements,
            constrained_sandbox_policy.get(),
        )
        .map_err(|err| {
            if let Some(source) = network_requirements_source.as_ref() {
                std::io::Error::new(
                    err.kind(),
                    format!("failed to build managed network proxy from {source}: {err}"),
                )
            } else {
                err
            }
        })?;
        let network = if has_network_requirements {
            Some(network)
        } else {
            network.enabled().then_some(network)
        };
        let helper_readable_roots = get_readable_roots_required_for_praxis_runtime(
            &praxis_home,
            zsh_path.as_ref(),
            main_execve_wrapper_exe.as_ref(),
        );
        let effective_sandbox_policy = constrained_sandbox_policy.value.get().clone();
        let effective_file_system_sandbox_policy =
            if effective_sandbox_policy == original_sandbox_policy {
                file_system_sandbox_policy
            } else {
                sandbox_projection::file_system_policy_from_sandbox_policy(
                    &effective_sandbox_policy,
                    resolved_cwd.as_path(),
                )
            };
        let effective_file_system_sandbox_policy = effective_file_system_sandbox_policy
            .with_additional_readable_roots(resolved_cwd.as_path(), &helper_readable_roots);
        let effective_network_sandbox_policy =
            if effective_sandbox_policy == original_sandbox_policy {
                network_sandbox_policy
            } else {
                sandbox_projection::network_policy_from_sandbox_policy(&effective_sandbox_policy)
            };
        let config = Self {
            model,
            service_tier,
            review_model,
            model_context_window: cfg.model_context_window,
            model_auto_compact_token_limit: cfg.model_auto_compact_token_limit,
            model_provider_id,
            model_provider,
            cwd: resolved_cwd,
            startup_warnings,
            permissions: Permissions {
                approval_policy: constrained_approval_policy.value,
                sandbox_policy: constrained_sandbox_policy.value,
                file_system_sandbox_policy: effective_file_system_sandbox_policy,
                network_sandbox_policy: effective_network_sandbox_policy,
                network,
                allow_login_shell,
                shell_environment_policy,
                windows_sandbox_mode,
                windows_sandbox_private_desktop,
            },
            approvals_reviewer,
            enforce_residency: enforce_residency.value,
            notify: cfg.notify,
            user_instructions,
            base_instructions,
            personality,
            developer_instructions,
            compact_prompt,
            commit_attribution,
            // The config.toml omits "_mode" because it's a config file. However, "_mode"
            // is important in code to differentiate the mode from the store implementation.
            cli_auth_credentials_store_mode: cfg.cli_auth_credentials_store.unwrap_or_default(),
            mcp_servers,
            plugin_marketplaces: cfg.plugin_marketplaces,
            // The config.toml omits "_mode" because it's a config file. However, "_mode"
            // is important in code to differentiate the mode from the store implementation.
            mcp_oauth_credentials_store_mode: cfg.mcp_oauth_credentials_store.unwrap_or_default(),
            mcp_oauth_callback_port: cfg.mcp_oauth_callback_port,
            mcp_oauth_callback_url: cfg.mcp_oauth_callback_url.clone(),
            model_providers,
            local_model_hosts: cfg.local_model_hosts,
            local_models: cfg.local_models,
            transcription: cfg.transcription.unwrap_or_default(),
            project_doc_max_bytes: cfg.project_doc_max_bytes.unwrap_or(PROJECT_DOC_MAX_BYTES),
            project_doc_fallback_filenames: cfg
                .project_doc_fallback_filenames
                .unwrap_or_default()
                .into_iter()
                .filter_map(|name| {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                })
                .collect(),
            tool_output_token_limit: cfg.tool_output_token_limit,
            agent_max_threads,
            agent_max_depth,
            agent_roles,
            memories: cfg.memories.unwrap_or_default().into(),
            agent_job_max_runtime_seconds,
            praxis_home,
            sqlite_home,
            log_dir,
            config_layer_stack,
            history,
            ephemeral: ephemeral.unwrap_or_default(),
            file_opener: cfg.file_opener.unwrap_or(UriBasedFileOpener::VsCode),
            praxis_self_exe,
            praxis_linux_sandbox_exe,
            main_execve_wrapper_exe,
            zsh_path,

            hide_agent_reasoning: cfg.hide_agent_reasoning.unwrap_or(false),
            show_raw_agent_reasoning: cfg
                .show_raw_agent_reasoning
                .or(show_raw_agent_reasoning)
                .unwrap_or(false),
            guardian_developer_instructions,
            model_reasoning_effort: config_profile
                .model_reasoning_effort
                .or(cfg.model_reasoning_effort),
            plan_mode_reasoning_effort: config_profile
                .plan_mode_reasoning_effort
                .or(cfg.plan_mode_reasoning_effort),
            model_reasoning_summary: config_profile
                .model_reasoning_summary
                .or(cfg.model_reasoning_summary),
            model_supports_reasoning_summaries: cfg.model_supports_reasoning_summaries,
            model_catalog,
            model_verbosity: config_profile.model_verbosity.or(cfg.model_verbosity),
            chatgpt_base_url: config_profile
                .chatgpt_base_url
                .or(cfg.chatgpt_base_url)
                .unwrap_or("https://chatgpt.com/backend-api/".to_string()),
            realtime_audio: cfg
                .audio
                .map_or_else(RealtimeAudioConfig::default, |audio| RealtimeAudioConfig {
                    microphone: audio.microphone,
                    speaker: audio.speaker,
                }),
            experimental_realtime_ws_base_url: cfg.experimental_realtime_ws_base_url,
            experimental_realtime_ws_model: cfg.experimental_realtime_ws_model,
            realtime: cfg
                .realtime
                .map_or_else(RealtimeConfig::default, |realtime| RealtimeConfig {
                    version: realtime.version.unwrap_or_default(),
                    session_type: realtime.session_type.unwrap_or_default(),
                }),
            experimental_realtime_ws_backend_prompt: cfg.experimental_realtime_ws_backend_prompt,
            experimental_realtime_ws_startup_context: cfg.experimental_realtime_ws_startup_context,
            experimental_realtime_start_instructions: cfg.experimental_realtime_start_instructions,
            forced_chatgpt_workspace_id,
            forced_login_method,
            include_apply_patch_tool: include_apply_patch_tool_flag,
            web_search_mode: constrained_web_search_mode.value,
            web_search_config,
            use_experimental_unified_exec_tool,
            background_terminal_max_timeout,
            ghost_snapshot,
            features,
            suppress_unstable_features_warning: cfg
                .suppress_unstable_features_warning
                .unwrap_or(false),
            active_profile: active_profile_name,
            active_project,
            windows_wsl_setup_acknowledged: cfg.windows_wsl_setup_acknowledged.unwrap_or(false),
            notices: cfg.notice.unwrap_or_default(),
            check_for_update_on_startup,
            disable_paste_burst: cfg.disable_paste_burst.unwrap_or(false),
            analytics_enabled: config_profile
                .analytics
                .as_ref()
                .and_then(|a| a.enabled)
                .or(cfg.analytics.as_ref().and_then(|a| a.enabled)),
            feedback_enabled: cfg
                .feedback
                .as_ref()
                .and_then(|feedback| feedback.enabled)
                .unwrap_or(true),
            tool_suggest,
            otel: {
                let t: OtelConfigToml = cfg.otel.unwrap_or_default();
                let log_user_prompt = t.log_user_prompt.unwrap_or(false);
                let environment = t
                    .environment
                    .unwrap_or(DEFAULT_OTEL_ENVIRONMENT.to_string());
                let exporter = t.exporter.unwrap_or(OtelExporterKind::None);
                let trace_exporter = t.trace_exporter.unwrap_or_else(|| exporter.clone());
                let metrics_exporter = t.metrics_exporter.unwrap_or(OtelExporterKind::Statsig);
                OtelConfig {
                    log_user_prompt,
                    environment,
                    exporter,
                    trace_exporter,
                    metrics_exporter,
                }
            },
        };
        Ok(config)
    }

    fn load_instructions(praxis_dir: Option<&Path>) -> Option<String> {
        let base = praxis_dir?;
        for candidate in [LOCAL_PROJECT_DOC_FILENAME, DEFAULT_PROJECT_DOC_FILENAME] {
            let mut path = base.to_path_buf();
            path.push(candidate);
            if let Ok(contents) = std::fs::read_to_string(&path) {
                let trimmed = contents.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    /// If `path` is `Some`, attempts to read the file at the given path and
    /// returns its contents as a trimmed `String`. If the file is empty, or
    /// is `Some` but cannot be read, returns an `Err`.
    fn try_read_non_empty_file(
        path: Option<&AbsolutePathBuf>,
        context: &str,
    ) -> std::io::Result<Option<String>> {
        let Some(path) = path else {
            return Ok(None);
        };

        let contents = std::fs::read_to_string(path).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!("failed to read {context} {}: {e}", path.display()),
            )
        })?;

        let s = contents.trim().to_string();
        if s.is_empty() {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{context} is empty: {}", path.display()),
            ))
        } else {
            Ok(Some(s))
        }
    }

    pub fn set_windows_sandbox_enabled(&mut self, value: bool) {
        self.permissions.windows_sandbox_mode = if value {
            Some(WindowsSandboxModeToml::Unelevated)
        } else if matches!(
            self.permissions.windows_sandbox_mode,
            Some(WindowsSandboxModeToml::Unelevated)
        ) {
            None
        } else {
            self.permissions.windows_sandbox_mode
        };
    }

    pub fn set_windows_elevated_sandbox_enabled(&mut self, value: bool) {
        self.permissions.windows_sandbox_mode = if value {
            Some(WindowsSandboxModeToml::Elevated)
        } else if matches!(
            self.permissions.windows_sandbox_mode,
            Some(WindowsSandboxModeToml::Elevated)
        ) {
            None
        } else {
            self.permissions.windows_sandbox_mode
        };
    }

    pub fn managed_network_requirements_enabled(&self) -> bool {
        self.config_layer_stack
            .requirements_toml()
            .network
            .is_some()
    }

    pub fn bundled_skills_enabled(&self) -> bool {
        crate::manager::bundled_skills_enabled_from_stack(&self.config_layer_stack)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

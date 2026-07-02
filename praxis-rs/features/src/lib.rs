//! Centralized feature flags and metadata.
//!
//! This crate defines the feature registry plus the logic used to resolve an
//! effective feature set from config-like inputs.

use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_otel::SessionTelemetry;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::WarningEvent;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use toml::Table;

/// High-level lifecycle stage for a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Features that are still under development, not ready for external use
    UnderDevelopment,
    /// Experimental features made available to users through the `/experimental` menu
    Experimental {
        name: &'static str,
        menu_description: &'static str,
        announcement: &'static str,
    },
    /// Stable features. The feature flag is kept for ad-hoc enabling/disabling
    Stable,
    /// Deprecated feature that should not be used anymore.
    Deprecated,
    /// Feature no longer has runtime effect and is hidden from active telemetry.
    Removed,
}

impl Stage {
    pub fn experimental_menu_name(self) -> Option<&'static str> {
        match self {
            Stage::Experimental { name, .. } => Some(name),
            Stage::UnderDevelopment | Stage::Stable | Stage::Deprecated | Stage::Removed => None,
        }
    }

    pub fn experimental_menu_description(self) -> Option<&'static str> {
        match self {
            Stage::Experimental {
                menu_description, ..
            } => Some(menu_description),
            Stage::UnderDevelopment | Stage::Stable | Stage::Deprecated | Stage::Removed => None,
        }
    }

    pub fn experimental_announcement(self) -> Option<&'static str> {
        match self {
            Stage::Experimental {
                announcement: "", ..
            } => None,
            Stage::Experimental { announcement, .. } => Some(announcement),
            Stage::UnderDevelopment | Stage::Stable | Stage::Deprecated | Stage::Removed => None,
        }
    }
}

/// Unique features toggled via configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    // Stable.
    /// Create a ghost commit at each turn.
    GhostCommit,
    /// Enable the default shell tool.
    ShellTool,
    /// Enable structured read-only workspace navigation tools.
    FileNavigation,

    // Experimental
    /// Enable a minimal JavaScript mode backed by Node's built-in vm runtime.
    CodeMode,
    /// Restrict model-visible tools to code mode entrypoints (`exec`, `wait`).
    CodeModeOnly,
    /// Use the single unified PTY-backed exec tool.
    UnifiedExec,
    /// Route shell tool execution through the zsh exec bridge.
    ShellZshFork,
    /// Include the freeform apply_patch tool.
    ApplyPatchFreeform,
    /// Allow exec tools to request additional permissions while staying sandboxed.
    ExecPermissionApprovals,
    /// Enable Claude-style lifecycle hooks loaded from hooks.json files.
    PraxisHooks,
    /// Expose the built-in request_permissions tool.
    RequestPermissionsTool,
    /// Allow the model to request web searches that fetch live content.
    WebSearchRequest,
    /// Allow the model to request web searches that fetch cached content.
    /// Takes precedence over `WebSearchRequest`.
    WebSearchCached,
    /// Use the legacy Landlock Linux sandbox fallback instead of the default
    /// bubblewrap pipeline.
    UseLegacyLandlock,
    /// Experimental shell snapshotting.
    ShellSnapshot,
    /// Enable git commit attribution guidance via model instructions.
    PraxisGitCommit,
    /// Enable runtime metrics snapshots via a manual reader.
    RuntimeMetrics,
    /// Enable thread lifecycle analytics emitted via the app-gateway analytics pipeline.
    GeneralAnalytics,
    /// Enable startup memory extraction and file-backed memory consolidation.
    MemoryTool,
    /// Append additional AGENTS.md guidance to user instructions.
    ChildAgentsMd,
    /// Allow the model to request `detail: "original"` image outputs on supported models.
    ImageDetailOriginal,
    /// Compress request bodies (zstd) when sending streaming requests to praxis-backend.
    EnableRequestCompression,
    /// Enable collab tools.
    Collab,
    /// Enable CSV-backed agent job tools.
    SpawnCsv,
    /// Enable apps.
    Apps,
    /// Enable the tool_search tool for apps.
    ToolSearch,
    /// Enable discoverable tool suggestions for apps.
    ToolSuggest,
    /// Enable plugins.
    Plugins,
    /// Allow the model to invoke the built-in image generation tool.
    ImageGeneration,
    /// Allow the model to invoke built-in authorized reverse-engineering tools.
    ReverseEngineering,
    /// Allow prompting and installing missing MCP dependencies.
    SkillMcpDependencyInstall,
    /// Prompt for missing skill env var dependencies.
    SkillEnvVarDependencyPrompt,
    /// Allow request_user_input in Default collaboration mode.
    DefaultModeRequestUserInput,
    /// Enable automatic review for approval prompts.
    GuardianApproval,
    /// Route MCP tool approval prompts through the MCP elicitation request path.
    ToolCallMcpElicitation,
    /// Enable personality selection in the TUI.
    Personality,
    /// Enable native artifact tools.
    Artifact,
    /// Enable Fast mode selection in the TUI and request layer.
    FastMode,
    /// Enable experimental realtime voice conversation mode in the TUI.
    RealtimeConversation,
    /// Prevent idle system sleep while a turn is actively running.
    PreventIdleSleep,
}

impl Feature {
    pub fn key(self) -> &'static str {
        self.info().key
    }

    pub fn stage(self) -> Stage {
        self.info().stage
    }

    pub fn default_enabled(self) -> bool {
        self.info().default_enabled
    }

    fn info(self) -> &'static FeatureSpec {
        FEATURES
            .iter()
            .find(|spec| spec.id == self)
            .unwrap_or_else(|| unreachable!("missing FeatureSpec for {self:?}"))
    }
}

/// Holds the effective set of enabled features.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Features {
    enabled: BTreeSet<Feature>,
}

#[derive(Debug, Clone, Default)]
pub struct FeatureOverrides {
    pub include_apply_patch_tool: Option<bool>,
    pub web_search_request: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FeatureConfigSource<'a> {
    pub features: Option<&'a FeaturesToml>,
}

impl FeatureOverrides {
    fn apply(self, features: &mut Features) {
        if let Some(enabled) = self.include_apply_patch_tool {
            features.set_enabled(Feature::ApplyPatchFreeform, enabled);
        }
        if let Some(enabled) = self.web_search_request {
            if enabled {
                features.enable(Feature::WebSearchRequest);
            } else {
                features.disable(Feature::WebSearchRequest);
            }
        }
    }
}

impl Features {
    /// Starts with built-in defaults.
    pub fn with_defaults() -> Self {
        let mut set = BTreeSet::new();
        for spec in FEATURES {
            if spec.default_enabled {
                set.insert(spec.id);
            }
        }
        Self { enabled: set }
    }

    pub fn enabled(&self, f: Feature) -> bool {
        self.enabled.contains(&f)
    }

    pub async fn apps_enabled(&self, auth_manager: Option<&AuthManager>) -> bool {
        if !self.enabled(Feature::Apps) {
            return false;
        }

        let auth = match auth_manager {
            Some(auth_manager) => auth_manager.auth().await,
            None => None,
        };
        self.apps_enabled_for_auth(auth.as_ref())
    }

    pub fn apps_enabled_cached(&self, auth_manager: Option<&AuthManager>) -> bool {
        let auth = auth_manager.and_then(AuthManager::auth_cached);
        self.apps_enabled_for_auth(auth.as_ref())
    }

    pub fn apps_enabled_for_auth(&self, auth: Option<&OpenAiAccountAuth>) -> bool {
        self.enabled(Feature::Apps) && auth.is_some_and(OpenAiAccountAuth::is_chatgpt_auth)
    }

    pub fn use_legacy_landlock(&self) -> bool {
        self.enabled(Feature::UseLegacyLandlock)
    }

    pub fn enable(&mut self, f: Feature) -> &mut Self {
        self.enabled.insert(f);
        self
    }

    pub fn disable(&mut self, f: Feature) -> &mut Self {
        self.enabled.remove(&f);
        self
    }

    pub fn set_enabled(&mut self, f: Feature, enabled: bool) -> &mut Self {
        if enabled {
            self.enable(f)
        } else {
            self.disable(f)
        }
    }

    pub fn emit_metrics(&self, otel: &SessionTelemetry) {
        for feature in FEATURES {
            if matches!(feature.stage, Stage::Removed) {
                continue;
            }
            if self.enabled(feature.id) != feature.default_enabled {
                otel.counter(
                    "praxis.feature.state",
                    /*inc*/ 1,
                    &[
                        ("feature", feature.key),
                        ("value", &self.enabled(feature.id).to_string()),
                    ],
                );
            }
        }
    }

    /// Apply a table of key -> bool toggles (e.g. from TOML).
    pub fn apply_map(&mut self, m: &BTreeMap<String, bool>) {
        for (k, v) in m {
            match feature_for_key(k) {
                Some(feat) => {
                    if *v {
                        self.enable(feat);
                    } else {
                        self.disable(feat);
                    }
                }
                None => {
                    tracing::warn!("unknown feature key in config: {k}");
                }
            }
        }
    }

    pub fn from_sources(
        base: FeatureConfigSource<'_>,
        profile: FeatureConfigSource<'_>,
        overrides: FeatureOverrides,
    ) -> Self {
        let mut features = Features::with_defaults();

        for source in [base, profile] {
            if let Some(feature_entries) = source.features {
                features.apply_map(&feature_entries.entries);
            }
        }

        overrides.apply(&mut features);
        features.normalize_dependencies();

        features
    }

    pub fn enabled_features(&self) -> Vec<Feature> {
        self.enabled.iter().copied().collect()
    }

    pub fn normalize_dependencies(&mut self) {
        if self.enabled(Feature::SpawnCsv) && !self.enabled(Feature::Collab) {
            self.enable(Feature::Collab);
        }
        if self.enabled(Feature::CodeModeOnly) && !self.enabled(Feature::CodeMode) {
            self.enable(Feature::CodeMode);
        }
    }
}

/// Keys accepted in `[features]` tables.
pub fn feature_for_key(key: &str) -> Option<Feature> {
    for spec in FEATURES {
        if spec.key == key {
            return Some(spec.id);
        }
    }
    None
}

pub fn canonical_feature_for_key(key: &str) -> Option<Feature> {
    FEATURES
        .iter()
        .find(|spec| spec.key == key)
        .map(|spec| spec.id)
}

/// Returns `true` if the provided string matches a known feature toggle key.
pub fn is_known_feature_key(key: &str) -> bool {
    feature_for_key(key).is_some()
}

/// Deserializable features table for TOML.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema)]
pub struct FeaturesToml {
    #[serde(flatten)]
    pub entries: BTreeMap<String, bool>,
}

/// Single, easy-to-read registry of all feature definitions.
#[derive(Debug, Clone, Copy)]
pub struct FeatureSpec {
    pub id: Feature,
    pub key: &'static str,
    pub stage: Stage,
    pub default_enabled: bool,
}

pub const FEATURES: &[FeatureSpec] = &[
    // Stable features.
    FeatureSpec {
        id: Feature::GhostCommit,
        key: "undo",
        stage: Stage::Stable,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ShellTool,
        key: "shell_tool",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::FileNavigation,
        key: "file_navigation",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::UnifiedExec,
        key: "unified_exec",
        stage: Stage::Stable,
        default_enabled: !cfg!(windows),
    },
    FeatureSpec {
        id: Feature::ShellZshFork,
        key: "shell_zsh_fork",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ShellSnapshot,
        key: "shell_snapshot",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::CodeMode,
        key: "code_mode",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::CodeModeOnly,
        key: "code_mode_only",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::WebSearchRequest,
        key: "web_search_request",
        stage: Stage::Deprecated,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::WebSearchCached,
        key: "web_search_cached",
        stage: Stage::Deprecated,
        default_enabled: false,
    },
    // Experimental program. Rendered in the `/experimental` menu for users.
    FeatureSpec {
        id: Feature::PraxisGitCommit,
        key: "praxis_git_commit",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::RuntimeMetrics,
        key: "runtime_metrics",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::GeneralAnalytics,
        key: "general_analytics",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::MemoryTool,
        key: "memories",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ChildAgentsMd,
        key: "child_agents_md",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ImageDetailOriginal,
        key: "image_detail_original",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ApplyPatchFreeform,
        key: "apply_patch_freeform",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ExecPermissionApprovals,
        key: "exec_permission_approvals",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::PraxisHooks,
        key: "praxis_hooks",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::RequestPermissionsTool,
        key: "request_permissions_tool",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::UseLegacyLandlock,
        key: "use_legacy_landlock",
        stage: Stage::Stable,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::EnableRequestCompression,
        key: "enable_request_compression",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Collab,
        key: "multi_agent",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::SpawnCsv,
        key: "enable_fanout",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::Apps,
        key: "apps",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::ToolSearch,
        key: "tool_search",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ToolSuggest,
        key: "tool_suggest",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Plugins,
        key: "plugins",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::ImageGeneration,
        key: "image_generation",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::ReverseEngineering,
        key: "reverse_engineering",
        stage: Stage::Experimental {
            name: "Reverse Engineering",
            menu_description: "Authorized reverse-engineering tools for local targets. Requires per-target user authorization and returns codec-filtered evidence only.",
            announcement: "",
        },
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::SkillMcpDependencyInstall,
        key: "skill_mcp_dependency_install",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::SkillEnvVarDependencyPrompt,
        key: "skill_env_var_dependency_prompt",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::DefaultModeRequestUserInput,
        key: "default_mode_request_user_input",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::GuardianApproval,
        key: "guardian_approval",
        stage: Stage::Experimental {
            name: "Guardian Approvals",
            menu_description: "When Praxis needs approval for higher-risk actions (e.g. sandbox escapes or blocked network access), route eligible approval requests to a carefully-prompted security reviewer subagent rather than blocking the agent on your input. This can consume significantly more tokens because it runs a subagent on every approval request.",
            announcement: "",
        },
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::ToolCallMcpElicitation,
        key: "tool_call_mcp_elicitation",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Personality,
        key: "personality",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::Artifact,
        key: "artifact",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::FastMode,
        key: "fast_mode",
        stage: Stage::Stable,
        default_enabled: true,
    },
    FeatureSpec {
        id: Feature::RealtimeConversation,
        key: "realtime_conversation",
        stage: Stage::UnderDevelopment,
        default_enabled: false,
    },
    FeatureSpec {
        id: Feature::PreventIdleSleep,
        key: "prevent_idle_sleep",
        stage: if cfg!(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )) {
            Stage::Experimental {
                name: "Prevent sleep while running",
                menu_description: "Keep your computer awake while Praxis is running a thread.",
                announcement: "NEW: Prevent sleep while running is now available in /experimental.",
            }
        } else {
            Stage::UnderDevelopment
        },
        default_enabled: false,
    },
];

pub fn unstable_features_warning_event(
    effective_features: Option<&Table>,
    suppress_unstable_features_warning: bool,
    features: &Features,
    config_path: &str,
) -> Option<Event> {
    if suppress_unstable_features_warning {
        return None;
    }

    let mut under_development_feature_keys = Vec::new();
    if let Some(table) = effective_features {
        for (key, value) in table {
            if value.as_bool() != Some(true) {
                continue;
            }
            let Some(spec) = FEATURES.iter().find(|spec| spec.key == key.as_str()) else {
                continue;
            };
            if !features.enabled(spec.id) {
                continue;
            }
            if matches!(spec.stage, Stage::UnderDevelopment) {
                under_development_feature_keys.push(spec.key.to_string());
            }
        }
    }

    if under_development_feature_keys.is_empty() {
        return None;
    }

    let under_development_feature_keys = under_development_feature_keys.join(", ");
    let message = format!(
        "Under-development features enabled: {under_development_feature_keys}. Under-development features are incomplete and may behave unpredictably. To suppress this warning, set `suppress_unstable_features_warning = true` in {config_path}."
    );
    Some(Event {
        id: String::new(),
        msg: EventMsg::Warning(WarningEvent { message }),
    })
}

#[cfg(test)]
mod tests;

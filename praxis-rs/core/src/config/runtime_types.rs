use super::service_types::Tools;
use praxis_config::types::SandboxWorkspaceWrite;
use praxis_config::types::ToolSuggestConfig;
use praxis_protocol::config_types::TrustLevel;
use praxis_protocol::config_types::WebSearchToolConfig;
use praxis_utils_absolute_path::AbsolutePathBuf;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ProjectConfig {
    pub trust_level: Option<TrustLevel>,
}

impl ProjectConfig {
    pub fn is_trusted(&self) -> bool {
        matches!(self.trust_level, Some(TrustLevel::Trusted))
    }

    pub fn is_untrusted(&self) -> bool {
        matches!(self.trust_level, Some(TrustLevel::Untrusted))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RealtimeAudioConfig {
    pub microphone: Option<String>,
    pub speaker: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalModelHostKind {
    ExternalHttp,
    ManagedServer,
    NativeEngine,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct LocalModelHostConfig {
    pub kind: LocalModelHostKind,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub health_path: Option<String>,
    pub model_path: Option<AbsolutePathBuf>,
    pub tokenizer_path: Option<AbsolutePathBuf>,
    pub idle_timeout_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

const DEFAULT_LOCAL_MODEL_SCAN_MAX_DEPTH: usize = 6;

fn default_local_model_scan_max_depth() -> usize {
    DEFAULT_LOCAL_MODEL_SCAN_MAX_DEPTH
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct LocalModelsConfig {
    #[serde(default)]
    pub paths: Vec<AbsolutePathBuf>,
    #[serde(default = "default_local_model_scan_max_depth")]
    #[schemars(range(min = 1))]
    pub scan_max_depth: usize,
}

impl Default for LocalModelsConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            scan_max_depth: DEFAULT_LOCAL_MODEL_SCAN_MAX_DEPTH,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionProviderKind {
    OpenAi,
    DashScopeQwen,
    LocalHttp,
    LocalProcess,
    NativeEngine,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionSubmitMode {
    #[default]
    InsertIntoComposer,
    AutoSubmit,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct TranscriptionProviderConfig {
    pub kind: TranscriptionProviderKind,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct TranscriptionConfig {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    #[serde(default)]
    pub submit_mode: TranscriptionSubmitMode,
    #[serde(default)]
    pub providers: BTreeMap<String, TranscriptionProviderConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeWsMode {
    #[default]
    Conversational,
    Transcription,
}

pub use praxis_protocol::protocol::RealtimeConversationVersion as RealtimeWsVersion;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct RealtimeConfig {
    pub version: RealtimeWsVersion,
    #[serde(rename = "type")]
    pub session_type: RealtimeWsMode,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct RealtimeToml {
    pub version: Option<RealtimeWsVersion>,
    #[serde(rename = "type")]
    pub session_type: Option<RealtimeWsMode>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct RealtimeAudioToml {
    pub microphone: Option<String>,
    pub speaker: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ToolsToml {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_web_search_tool_config"
    )]
    pub web_search: Option<WebSearchToolConfig>,

    pub view_image: Option<bool>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WebSearchToolConfigInput {
    Enabled(bool),
    Config(WebSearchToolConfig),
}

fn deserialize_optional_web_search_tool_config<'de, D>(
    deserializer: D,
) -> Result<Option<WebSearchToolConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<WebSearchToolConfigInput>::deserialize(deserializer)?;

    Ok(match value {
        None => None,
        Some(WebSearchToolConfigInput::Enabled(enabled)) => {
            let _ = enabled;
            None
        }
        Some(WebSearchToolConfigInput::Config(config)) => Some(config),
    })
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AgentsToml {
    #[schemars(range(min = 1))]
    pub max_threads: Option<usize>,
    #[schemars(range(min = 1))]
    pub max_depth: Option<i32>,
    #[schemars(range(min = 1))]
    pub job_max_runtime_seconds: Option<u64>,
    #[serde(default, flatten)]
    pub roles: BTreeMap<String, AgentRoleToml>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentRoleConfig {
    pub description: Option<String>,
    pub config_file: Option<PathBuf>,
    pub base_name_candidates: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AgentRoleToml {
    pub description: Option<String>,
    pub config_file: Option<AbsolutePathBuf>,
    pub base_name_candidates: Option<Vec<String>>,
}

impl From<ToolsToml> for Tools {
    fn from(tools_toml: ToolsToml) -> Self {
        Self {
            web_search: tools_toml.web_search,
            view_image: tools_toml.view_image,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct GhostSnapshotToml {
    #[serde(alias = "ignore_untracked_files_over_bytes")]
    pub ignore_large_untracked_files: Option<i64>,
    #[serde(alias = "large_untracked_dir_warning_threshold")]
    pub ignore_large_untracked_dirs: Option<i64>,
    pub disable_warnings: Option<bool>,
}

pub(super) fn sandbox_settings_from_workspace_write(
    sandbox_workspace_write: SandboxWorkspaceWrite,
) -> super::SandboxSettings {
    super::SandboxSettings {
        writable_roots: sandbox_workspace_write.writable_roots,
        network_access: Some(sandbox_workspace_write.network_access),
        exclude_tmpdir_env_var: Some(sandbox_workspace_write.exclude_tmpdir_env_var),
        exclude_slash_tmp: Some(sandbox_workspace_write.exclude_slash_tmp),
    }
}

pub(super) fn resolve_tool_suggest_config(config_toml: &super::ConfigToml) -> ToolSuggestConfig {
    let discoverables = config_toml
        .tool_suggest
        .as_ref()
        .into_iter()
        .flat_map(|tool_suggest| tool_suggest.discoverables.iter())
        .filter_map(|discoverable| {
            let trimmed = discoverable.id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(praxis_config::types::ToolSuggestDiscoverable {
                    kind: discoverable.kind,
                    id: trimmed.to_string(),
                })
            }
        })
        .collect();

    ToolSuggestConfig { discoverables }
}

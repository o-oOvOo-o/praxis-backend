use std::collections::HashMap;

use praxis_config::types::ApprovalsReviewer;
use praxis_config::types::AppsConfigToml;
use praxis_protocol::config_layers::ConfigLayer;
use praxis_protocol::config_layers::ConfigLayerMetadata;
use praxis_protocol::config_types::ForcedLoginMethod;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::SandboxMode;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::config_types::Verbosity;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::config_types::WebSearchToolConfig;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::AskForApproval;
use praxis_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;

pub use praxis_config::types::AppConfig;
pub use praxis_config::types::AppToolApproval;
pub use praxis_config::types::AppsConfigToml as AppsConfig;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AnalyticsConfig {
    pub enabled: Option<bool>,
    #[serde(default, flatten)]
    pub additional: HashMap<String, JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub struct SandboxSettings {
    #[serde(default)]
    pub writable_roots: Vec<AbsolutePathBuf>,
    pub network_access: Option<bool>,
    pub exclude_tmpdir_env_var: Option<bool>,
    pub exclude_slash_tmp: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Tools {
    pub web_search: Option<WebSearchToolConfig>,
    pub view_image: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Profile {
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub approval_policy: Option<AskForApproval>,
    pub approvals_reviewer: Option<ApprovalsReviewer>,
    pub service_tier: Option<ServiceTier>,
    pub model_reasoning_effort: Option<ReasoningEffort>,
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub model_verbosity: Option<Verbosity>,
    pub web_search: Option<WebSearchMode>,
    pub tools: Option<Tools>,
    pub chatgpt_base_url: Option<String>,
    #[serde(default, flatten)]
    pub additional: HashMap<String, JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserSavedConfig {
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    pub sandbox_settings: Option<SandboxSettings>,
    pub forced_chatgpt_workspace_id: Option<String>,
    pub forced_login_method: Option<ForcedLoginMethod>,
    pub model: Option<String>,
    pub model_reasoning_effort: Option<ReasoningEffort>,
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub model_verbosity: Option<Verbosity>,
    pub tools: Option<Tools>,
    pub profile: Option<String>,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConfigView {
    pub model: Option<String>,
    pub review_model: Option<String>,
    pub model_context_window: Option<i64>,
    pub model_auto_compact_token_limit: Option<i64>,
    pub model_provider: Option<String>,
    pub approval_policy: Option<AskForApproval>,
    pub approvals_reviewer: Option<ApprovalsReviewer>,
    pub sandbox_mode: Option<SandboxMode>,
    pub sandbox_workspace_write: Option<praxis_config::types::SandboxWorkspaceWrite>,
    pub forced_chatgpt_workspace_id: Option<String>,
    pub forced_login_method: Option<ForcedLoginMethod>,
    pub web_search: Option<WebSearchMode>,
    pub tools: Option<Tools>,
    pub profile: Option<String>,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
    pub instructions: Option<String>,
    pub developer_instructions: Option<String>,
    pub compact_prompt: Option<String>,
    pub model_reasoning_effort: Option<ReasoningEffort>,
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub model_verbosity: Option<Verbosity>,
    pub service_tier: Option<ServiceTier>,
    pub analytics: Option<AnalyticsConfig>,
    #[serde(default)]
    pub apps: Option<AppsConfigToml>,
    #[serde(default, flatten)]
    pub additional: HashMap<String, JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MergeStrategy {
    Replace,
    Upsert,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WriteStatus {
    Ok,
    OkOverridden,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OverriddenMetadata {
    pub message: String,
    pub overriding_layer: ConfigLayerMetadata,
    pub effective_value: JsonValue,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWriteResponse {
    pub status: WriteStatus,
    pub version: String,
    pub file_path: AbsolutePathBuf,
    pub overridden_metadata: Option<OverriddenMetadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConfigWriteErrorCode {
    ConfigLayerReadonly,
    ConfigVersionConflict,
    ConfigValidationError,
    ConfigPathNotFound,
    ConfigSchemaUnknownKey,
    UserLayerNotFound,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReadParams {
    #[serde(default)]
    pub include_layers: bool,
    pub cwd: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReadResponse {
    pub config: ConfigView,
    pub origins: HashMap<String, ConfigLayerMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<ConfigLayer>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigValueWriteParams {
    pub key_path: String,
    pub value: JsonValue,
    pub merge_strategy: MergeStrategy,
    pub file_path: Option<String>,
    pub expected_version: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigBatchWriteParams {
    pub edits: Vec<ConfigWriteEdit>,
    pub file_path: Option<String>,
    pub expected_version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reload_user_config: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWriteEdit {
    pub key_path: String,
    pub value: JsonValue,
    pub merge_strategy: MergeStrategy,
}

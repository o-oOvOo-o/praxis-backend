use super::*;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type")]
pub enum Account {
    #[serde(rename = "apiKey", rename_all = "camelCase")]
    #[ts(rename = "apiKey", rename_all = "camelCase")]
    ApiKey {},

    #[serde(rename = "chatgpt", rename_all = "camelCase")]
    #[ts(rename = "chatgpt", rename_all = "camelCase")]
    Chatgpt { email: String, plan_type: PlanType },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS, ExperimentalApi)]
#[serde(tag = "type")]
#[ts(tag = "type")]
pub enum LoginAccountParams {
    #[serde(rename = "apiKey", rename_all = "camelCase")]
    #[ts(rename = "apiKey", rename_all = "camelCase")]
    ApiKey {
        #[serde(rename = "apiKey")]
        #[ts(rename = "apiKey")]
        api_key: String,
    },
    #[serde(rename = "chatgpt")]
    #[ts(rename = "chatgpt")]
    Chatgpt,
    #[serde(rename = "chatgptDeviceCode")]
    #[ts(rename = "chatgptDeviceCode")]
    ChatgptDeviceCode,
    /// [UNSTABLE] FOR OPENAI INTERNAL USE ONLY - DO NOT USE.
    /// The access token must contain the same scopes that Praxis-managed ChatGPT auth tokens have.
    #[experimental("account/login/start.chatgptAuthTokens")]
    #[serde(rename = "chatgptAuthTokens", rename_all = "camelCase")]
    #[ts(rename = "chatgptAuthTokens", rename_all = "camelCase")]
    ChatgptAuthTokens {
        /// Access token (JWT) supplied by the client.
        /// This token is used for backend API requests and email extraction.
        access_token: String,
        /// Workspace/account identifier supplied by the client.
        chatgpt_account_id: String,
        /// Optional plan type supplied by the client.
        ///
        /// When `null`, Praxis attempts to derive the plan type from access-token
        /// claims. If unavailable, the plan defaults to `unknown`.
        #[ts(optional = nullable)]
        chatgpt_plan_type: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type")]
pub enum LoginAccountResponse {
    #[serde(rename = "apiKey", rename_all = "camelCase")]
    #[ts(rename = "apiKey", rename_all = "camelCase")]
    ApiKey {},
    #[serde(rename = "chatgpt", rename_all = "camelCase")]
    #[ts(rename = "chatgpt", rename_all = "camelCase")]
    Chatgpt {
        // Use plain String for identifiers to avoid TS/JSON Schema quirks around uuid-specific types.
        // Convert to/from UUIDs at the application layer as needed.
        login_id: String,
        /// URL the client should open in a browser to initiate the OAuth flow.
        auth_url: String,
    },
    #[serde(rename = "chatgptDeviceCode", rename_all = "camelCase")]
    #[ts(rename = "chatgptDeviceCode", rename_all = "camelCase")]
    ChatgptDeviceCode {
        // Use plain String for identifiers to avoid TS/JSON Schema quirks around uuid-specific types.
        // Convert to/from UUIDs at the application layer as needed.
        login_id: String,
        /// URL the client should open in a browser to complete device code authorization.
        verification_url: String,
        /// One-time code the user must enter after signing in.
        user_code: String,
    },
    #[serde(rename = "chatgptAuthTokens", rename_all = "camelCase")]
    #[ts(rename = "chatgptAuthTokens", rename_all = "camelCase")]
    ChatgptAuthTokens {},
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelLoginAccountParams {
    pub login_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum CancelLoginAccountStatus {
    Canceled,
    NotFound,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelLoginAccountResponse {
    pub status: CancelLoginAccountStatus,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct LogoutAccountResponse {}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ChatgptAuthTokensRefreshReason {
    /// Praxis attempted a backend request and received `401 Unauthorized`.
    Unauthorized,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChatgptAuthTokensRefreshParams {
    pub reason: ChatgptAuthTokensRefreshReason,
    /// Workspace/account identifier that Praxis was previously using.
    ///
    /// Clients that manage multiple accounts/workspaces can use this as a hint
    /// to refresh the token for the correct workspace.
    ///
    /// This may be `null` when the prior auth state did not include a workspace
    /// identifier (`chatgpt_account_id`).
    #[ts(optional = nullable)]
    pub previous_account_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChatgptAuthTokensRefreshResponse {
    pub access_token: String,
    pub chatgpt_account_id: String,
    pub chatgpt_plan_type: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountRateLimitsResponse {
    /// Multi-bucket view keyed by metered `limit_id`.
    pub rate_limits: HashMap<String, RateLimitSnapshot>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountParams {
    /// When `true`, requests a proactive token refresh before returning.
    ///
    /// In managed auth mode this triggers the normal refresh-token flow. In
    /// external auth mode this flag is ignored. Clients should refresh tokens
    /// themselves and call `account/login/start` with `chatgptAuthTokens`.
    #[serde(default)]
    pub refresh_token: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountResponse {
    pub account: Option<Account>,
    pub requires_openai_auth: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelListParams {
    /// Opaque pagination cursor returned by a previous call.
    #[ts(optional = nullable)]
    pub cursor: Option<String>,
    /// Optional page size; defaults to a reasonable server-side value.
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
    /// When true, include models that are hidden from the default picker list.
    #[ts(optional = nullable)]
    pub include_hidden: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelAvailabilityNux {
    pub message: String,
}

impl From<CoreModelAvailabilityNux> for ModelAvailabilityNux {
    fn from(value: CoreModelAvailabilityNux) -> Self {
        Self {
            message: value.message,
        }
    }
}

const fn default_model_supports_streaming() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderWireApi {
    Responses,
    Claude,
    OpenAiCompat,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub model_provider: Option<String>,
    pub model_provider_display_name: String,
    pub model_provider_wire_api: ModelProviderWireApi,
    pub model: String,
    pub upgrade: Option<String>,
    pub upgrade_info: Option<ModelUpgradeInfo>,
    pub availability_nux: Option<ModelAvailabilityNux>,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    pub supported_reasoning_efforts: Vec<ReasoningEffortOption>,
    pub default_reasoning_effort: ReasoningEffort,
    #[serde(default = "default_input_modalities")]
    pub input_modalities: Vec<InputModality>,
    #[serde(default)]
    pub supports_personality: bool,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default = "default_model_supports_streaming")]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_parallel_tool_calls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<i64>,
    // Only one model should be marked as default.
    pub is_default: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelUpgradeInfo {
    pub model: String,
    pub upgrade_copy: Option<String>,
    pub model_link: Option<String>,
    pub migration_markdown: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningEffortOption {
    pub reasoning_effort: ReasoningEffort,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub display_name: Option<String>,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelListResponse {
    pub data: Vec<Model>,
    /// Opaque cursor to pass to the next call to continue after the last item.
    /// If None, there are no more items to return.
    pub next_cursor: Option<String>,
}

/// EXPERIMENTAL - list collaboration mode presets.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationModeListParams {}

/// EXPERIMENTAL - collaboration mode preset metadata for clients.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationModeMask {
    pub name: String,
    pub mode: Option<ModeKind>,
    pub model: Option<String>,
    #[serde(rename = "reasoning_effort")]
    #[ts(rename = "reasoning_effort")]
    pub reasoning_effort: Option<Option<ReasoningEffort>>,
}

impl From<CoreCollaborationModeMask> for CollaborationModeMask {
    fn from(value: CoreCollaborationModeMask) -> Self {
        Self {
            name: value.name,
            mode: value.mode,
            model: value.model,
            reasoning_effort: value.reasoning_effort,
        }
    }
}

/// EXPERIMENTAL - collaboration mode presets response.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationModeListResponse {
    pub data: Vec<CollaborationModeMask>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalFeatureListParams {
    /// Opaque pagination cursor returned by a previous call.
    #[ts(optional = nullable)]
    pub cursor: Option<String>,
    /// Optional page size; defaults to a reasonable server-side value.
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ExperimentalFeatureStage {
    /// Feature is available for user testing and feedback.
    Beta,
    /// Feature is still being built and not ready for broad use.
    UnderDevelopment,
    /// Feature is production-ready.
    Stable,
    /// Feature is deprecated and should be avoided.
    Deprecated,
    /// Feature flag is retained only for backwards compatibility.
    Removed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalFeature {
    /// Stable key used in config.toml and CLI flag toggles.
    pub name: String,
    /// Lifecycle stage of this feature flag.
    pub stage: ExperimentalFeatureStage,
    /// User-facing display name shown in the experimental features UI.
    /// Null when this feature is not in beta.
    pub display_name: Option<String>,
    /// Short summary describing what the feature does.
    /// Null when this feature is not in beta.
    pub description: Option<String>,
    /// Announcement copy shown to users when the feature is introduced.
    /// Null when this feature is not in beta.
    pub announcement: Option<String>,
    /// Whether this feature is currently enabled in the loaded config.
    pub enabled: bool,
    /// Whether this feature is enabled by default.
    pub default_enabled: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalFeatureListResponse {
    pub data: Vec<ExperimentalFeature>,
    /// Opaque cursor to pass to the next call to continue after the last item.
    /// If None, there are no more items to return.
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalFeatureEnablementSetParams {
    /// Process-wide runtime feature enablement keyed by canonical feature name.
    ///
    /// Only named features are updated. Omitted features are left unchanged.
    /// Send an empty map for a no-op.
    pub enablement: std::collections::BTreeMap<String, bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalFeatureEnablementSetResponse {
    /// Feature enablement entries updated by this request.
    pub enablement: std::collections::BTreeMap<String, bool>,
}

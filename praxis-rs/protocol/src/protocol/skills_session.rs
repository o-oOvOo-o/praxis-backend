use super::*;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[ts(type = "string")]
pub struct Product(String);

impl Product {
    pub const CHATGPT: &'static str = "chatgpt";
    pub const PRAXIS: &'static str = "praxis";
    pub const ATLAS: &'static str = "atlas";
    pub const CUNNING3D: &'static str = "cunning3d";

    pub fn new(value: impl AsRef<str>) -> Result<Self, String> {
        let normalized = canonical_product_id(value.as_ref())?;
        Ok(Self(normalized))
    }

    pub fn chatgpt() -> Self {
        Self(Self::CHATGPT.to_string())
    }

    pub fn praxis() -> Self {
        Self(Self::PRAXIS.to_string())
    }

    pub fn atlas() -> Self {
        Self(Self::ATLAS.to_string())
    }

    pub fn cunning3d() -> Self {
        Self(Self::CUNNING3D.to_string())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn is_praxis(&self) -> bool {
        self.as_str() == Self::PRAXIS
    }

    pub fn to_app_platform(&self) -> &str {
        match self.as_str() {
            Self::CHATGPT => "chat",
            other => other,
        }
    }

    pub fn from_session_source_name(value: &str) -> Option<Self> {
        Self::new(value).ok()
    }

    pub fn matches_product_restriction(&self, products: &[Product]) -> bool {
        products.is_empty() || products.contains(self)
    }
}

impl TryFrom<String> for Product {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for Product {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Product> for String {
    fn from(value: Product) -> Self {
        value.0
    }
}

impl FromStr for Product {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for Product {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Product {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for Product {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn canonical_product_id(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("product id must not be empty".to_string());
    }
    let canonical = match normalized.as_str() {
        "codex" => Product::PRAXIS,
        "c3d" | "cunning3d-desktop" | "cunning3d_desktop" => Product::CUNNING3D,
        other => other,
    };
    validate_product_id(canonical)?;
    Ok(canonical.to_string())
}

fn validate_product_id(value: &str) -> Result<(), String> {
    if value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        Ok(())
    } else {
        Err("product id must use lowercase letters, digits, '-' or '_'".to_string())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum SkillScope {
    User,
    Repo,
    System,
    Admin,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    /// Legacy short_description from SKILL.md. Prefer SKILL.json interface.short_description.
    pub short_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interface: Option<SkillInterface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dependencies: Option<SkillDependencies>,
    pub path: PathBuf,
    pub scope: SkillScope,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS, PartialEq, Eq)]
pub struct SkillInterface {
    #[ts(optional)]
    pub display_name: Option<String>,
    #[ts(optional)]
    pub short_description: Option<String>,
    #[ts(optional)]
    pub icon_small: Option<PathBuf>,
    #[ts(optional)]
    pub icon_large: Option<PathBuf>,
    #[ts(optional)]
    pub brand_color: Option<String>,
    #[ts(optional)]
    pub default_prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS, PartialEq, Eq)]
pub struct SkillDependencies {
    pub tools: Vec<SkillToolDependency>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS, PartialEq, Eq)]
pub struct SkillToolDependency {
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub r#type: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
pub struct SkillErrorInfo {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
pub struct SkillsListEntry {
    pub cwd: PathBuf,
    pub skills: Vec<SkillMetadata>,
    pub errors: Vec<SkillErrorInfo>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS, PartialEq, Eq)]
pub struct SessionNetworkProxyRuntime {
    pub http_addr: String,
    pub socks_addr: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
pub struct SessionConfiguredEvent {
    pub session_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<ThreadId>,

    /// Optional user-facing thread name (may be unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thread_name: Option<String>,

    /// Tell the client what model is being queried.
    pub model: String,

    pub model_provider_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,

    /// When to escalate for approval for execution
    pub approval_policy: AskForApproval,

    /// Configures who approval requests are routed to for review once they have
    /// been escalated. This does not disable separate safety checks such as
    /// ARC.
    #[serde(default)]
    pub approvals_reviewer: ApprovalsReviewer,

    /// How to sandbox commands executed in the system
    pub sandbox_policy: SandboxPolicy,

    /// Working directory that should be treated as the *root* of the
    /// session.
    pub cwd: PathBuf,

    /// The effort the model is putting into reasoning about the user's request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffortConfig>,

    /// Identifier of the history log file (inode on Unix, 0 otherwise).
    pub history_log_id: u64,

    /// Current number of entries in the history log.
    pub history_entry_count: usize,

    /// Optional initial messages (as events) for resumed sessions.
    /// When present, UIs can use these to seed the history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<EventMsg>>,

    /// Runtime proxy bind addresses, when the managed proxy was started for this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub network_proxy: Option<SessionNetworkProxyRuntime>,

    /// Path in which the rollout is stored. Can be `None` for ephemeral threads
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollout_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
pub struct ThreadNameUpdatedEvent {
    pub thread_id: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thread_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "protocol/")]
pub enum ThreadGoalStatus {
    Active,
    Paused,
    Blocked,
    UsageLimited,
    BudgetLimited,
    Complete,
}

pub const MAX_THREAD_GOAL_OBJECTIVE_CHARS: usize = 4_000;

pub fn validate_thread_goal_objective(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("goal objective must not be empty".to_string());
    }
    if value.chars().count() > MAX_THREAD_GOAL_OBJECTIVE_CHARS {
        return Err(format!(
            "goal objective must be at most {MAX_THREAD_GOAL_OBJECTIVE_CHARS} characters"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "protocol/")]
pub struct ThreadGoal {
    pub thread_id: ThreadId,
    pub objective: String,
    pub status: ThreadGoalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub token_budget: Option<i64>,
    pub tokens_used: i64,
    pub time_used_seconds: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "protocol/")]
pub struct ThreadHeartbeat {
    pub thread_id: ThreadId,
    pub enabled: bool,
    pub interval_ms: i64,
    pub next_wake_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_wake_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub controller: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "protocol/")]
pub struct ThreadGoalUpdatedEvent {
    pub thread_id: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    pub goal: ThreadGoal,
}

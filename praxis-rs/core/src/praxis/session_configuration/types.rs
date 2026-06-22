use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionSource;
use praxis_utils_absolute_path::AbsolutePathBuf;
use serde_json::Value;

use crate::ModelProviderInfo;
use crate::config::Config;
use crate::config::Constrained;
use crate::shell;
use crate::shell_snapshot::ShellSnapshot;

/// Notes from the previous real user turn.
///
/// Conceptually this is the same role that `previous_model` used to fill, but
/// it can carry other prior-turn settings that matter when constructing
/// sensible state-change diffs or full-context reinjection, such as model
/// switches or detecting a prior `realtime_active -> false` transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviousTurnSettings {
    pub(crate) model: String,
    pub(crate) realtime_active: Option<bool>,
}

#[derive(Clone)]
pub(crate) struct SessionConfiguration {
    /// Provider identifier ("openai", "openrouter", ...).
    pub(in crate::praxis) provider: ModelProviderInfo,

    pub(in crate::praxis) collaboration_mode: CollaborationMode,
    pub(in crate::praxis) model_reasoning_summary: Option<ReasoningSummaryConfig>,
    pub(in crate::praxis) service_tier: Option<ServiceTier>,

    /// Developer instructions that supplement the base instructions.
    pub(in crate::praxis) developer_instructions: Option<String>,

    /// Model instructions that are appended to the base instructions.
    pub(in crate::praxis) user_instructions: Option<String>,

    /// Personality preference for the model.
    pub(in crate::praxis) personality: Option<Personality>,

    /// Base instructions for the session.
    pub(in crate::praxis) base_instructions: String,

    /// Compact prompt override.
    pub(in crate::praxis) compact_prompt: Option<String>,

    /// When to escalate for approval for execution.
    pub(in crate::praxis) approval_policy: Constrained<AskForApproval>,
    pub(in crate::praxis) approvals_reviewer: ApprovalsReviewer,

    /// How to sandbox commands executed in the system.
    pub(in crate::praxis) sandbox_policy: Constrained<SandboxPolicy>,
    pub(in crate::praxis) file_system_sandbox_policy: FileSystemSandboxPolicy,
    pub(in crate::praxis) network_sandbox_policy: NetworkSandboxPolicy,
    pub(in crate::praxis) windows_sandbox_level: WindowsSandboxLevel,

    /// Absolute working directory that should be treated as the root of the session.
    pub(in crate::praxis) cwd: AbsolutePathBuf,

    /// Directory containing all Praxis state for this session.
    pub(in crate::praxis) praxis_home: PathBuf,

    /// Optional user-facing name for the thread, updated during the session.
    pub(in crate::praxis) thread_name: Option<String>,

    // TODO(pakrym): Remove config from here.
    pub(in crate::praxis) original_config_do_not_use: Arc<Config>,

    /// Optional service name tag for session metrics.
    pub(in crate::praxis) metrics_service_name: Option<String>,
    pub(in crate::praxis) app_gateway_client_name: Option<String>,

    /// Source of the session (cli, vscode, exec, mcp, ...).
    pub(in crate::praxis) session_source: SessionSource,
    pub(in crate::praxis) dynamic_tools: Vec<DynamicToolSpec>,
    pub(in crate::praxis) persist_extended_history: bool,
    pub(in crate::praxis) inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
    pub(in crate::praxis) user_shell_override: Option<shell::Shell>,
}

#[derive(Default, Clone)]
pub(crate) struct SessionSettingsUpdate {
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) approval_policy: Option<AskForApproval>,
    pub(crate) approvals_reviewer: Option<ApprovalsReviewer>,
    pub(crate) sandbox_policy: Option<SandboxPolicy>,
    pub(crate) windows_sandbox_level: Option<WindowsSandboxLevel>,
    pub(crate) model_provider: Option<String>,
    pub(crate) collaboration_mode: Option<CollaborationMode>,
    pub(crate) reasoning_summary: Option<ReasoningSummaryConfig>,
    pub(crate) service_tier: Option<Option<ServiceTier>>,
    pub(crate) final_output_json_schema: Option<Option<Value>>,
    pub(crate) personality: Option<Personality>,
    pub(crate) app_gateway_client_name: Option<String>,
}

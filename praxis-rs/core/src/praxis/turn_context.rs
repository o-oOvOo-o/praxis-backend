use std::path::PathBuf;
use std::sync::Arc;

use praxis_exec_server::Environment;
use praxis_login::AuthManager;
use praxis_network_proxy::NetworkProxy;
use praxis_otel::SessionTelemetry;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SessionSource;
use praxis_tools::ToolsConfig;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_output_truncation::TruncationPolicy;
use praxis_utils_readiness::ReadinessFlag;
use serde_json::Value;

use crate::ModelProviderInfo;
use crate::compact;
use crate::config::Config;
use crate::config::GhostSnapshotConfig;
use crate::config::ManagedFeatures;
use crate::tools::loop_guard::ToolLoopGuardState;
use crate::turn_metadata::TurnMetadataState;
use crate::turn_timing::TurnTimingState;

use super::EffectivePermissions;
use super::LiveEffectivePermissions;
use super::TurnSkillsContext;

mod assembly;
mod builder;
mod config_access;
mod effective_permissions;
mod mcp_sandbox;
mod model_variant;
mod per_turn_config;
mod prompt_instructions;
mod protocol_projection;
mod settings;
mod settings_update_items;
mod shell_snapshot_refresh;
mod tools_config;
mod turn_skills;

/// The context needed for a single turn of the thread.
#[derive(Debug)]
pub(crate) struct TurnContext {
    pub(crate) sub_id: String,
    pub(crate) trace_id: Option<String>,
    pub(crate) realtime_active: bool,
    pub(crate) config: Arc<Config>,
    pub(crate) auth_manager: Option<Arc<AuthManager>>,
    pub(crate) model_info: ModelInfo,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) reasoning_effort: Option<ReasoningEffortConfig>,
    pub(crate) reasoning_summary: ReasoningSummaryConfig,
    pub(crate) session_source: SessionSource,
    pub(crate) environment: Arc<Environment>,
    /// The session's absolute working directory.
    pub(crate) cwd: AbsolutePathBuf,
    pub(crate) current_date: Option<String>,
    pub(crate) timezone: Option<String>,
    pub(crate) app_gateway_client_name: Option<String>,
    pub(crate) developer_instructions: Option<String>,
    pub(crate) compact_prompt: Option<String>,
    pub(crate) user_instructions: Option<String>,
    pub(crate) collaboration_mode: CollaborationMode,
    pub(crate) personality: Option<Personality>,
    pub(crate) effective_permissions: LiveEffectivePermissions,
    pub(crate) network: Option<NetworkProxy>,
    pub(crate) shell_environment_policy: praxis_config::types::ShellEnvironmentPolicy,
    pub(crate) tools_config: ToolsConfig,
    pub(crate) features: ManagedFeatures,
    pub(crate) ghost_snapshot: GhostSnapshotConfig,
    pub(crate) final_output_json_schema: Option<Value>,
    pub(crate) praxis_self_exe: Option<PathBuf>,
    pub(crate) praxis_linux_sandbox_exe: Option<PathBuf>,
    pub(crate) tool_call_gate: Arc<ReadinessFlag>,
    pub(crate) tool_loop_guard: Arc<ToolLoopGuardState>,
    pub(crate) truncation_policy: TruncationPolicy,
    pub(crate) dynamic_tools: Vec<DynamicToolSpec>,
    pub(crate) turn_metadata_state: Arc<TurnMetadataState>,
    pub(crate) turn_skills: TurnSkillsContext,
    pub(crate) turn_timing_state: Arc<TurnTimingState>,
}

impl TurnContext {
    pub(crate) fn effective_permissions(&self) -> EffectivePermissions {
        self.effective_permissions.snapshot()
    }

    pub(crate) fn effective_approval_policy(&self) -> AskForApproval {
        self.effective_permissions().approval_policy.value()
    }

    pub(crate) fn model_context_window(&self) -> Option<i64> {
        let effective_context_window_percent = self.model_info.effective_context_window_percent;
        self.model_info.context_window.map(|context_window| {
            context_window.saturating_mul(effective_context_window_percent) / 100
        })
    }

    pub(crate) fn apps_enabled(&self) -> bool {
        self.features
            .apps_enabled_cached(self.auth_manager.as_deref())
    }

    pub(crate) fn resolve_path(&self, path: Option<String>) -> PathBuf {
        path.as_ref()
            .map(PathBuf::from)
            .map_or_else(|| self.cwd.to_path_buf(), |p| self.cwd.as_path().join(p))
    }

    pub(crate) fn compact_prompt(&self) -> &str {
        self.compact_prompt
            .as_deref()
            .unwrap_or(compact::SUMMARIZATION_PROMPT)
    }
}

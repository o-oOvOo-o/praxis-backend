use std::sync::Arc;

use praxis_otel::current_span_trace_id;
use praxis_protocol::openai_models::ModelInfo;
use praxis_tools::ToolsConfig;
use praxis_utils_readiness::ReadinessFlag;

use crate::config::Config;
use crate::tools::loop_guard::ToolLoopGuardState;
use crate::turn_metadata::TurnMetadataState;
use crate::turn_timing::TurnTimingState;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::super::TurnSkillsContext;

pub(super) struct ReviewTurnContextAssemblyInput<'a> {
    pub(super) session: &'a Arc<Session>,
    pub(super) parent_turn_context: &'a Arc<TurnContext>,
    pub(super) model: String,
    pub(super) model_info: ModelInfo,
    pub(super) per_turn_config: Config,
    pub(super) tools_config: ToolsConfig,
    pub(super) review_turn_id: String,
}

pub(super) fn build(input: ReviewTurnContextAssemblyInput<'_>) -> Arc<TurnContext> {
    let ReviewTurnContextAssemblyInput {
        session,
        parent_turn_context,
        model,
        model_info,
        per_turn_config,
        tools_config,
        review_turn_id,
    } = input;
    let permissions = parent_turn_context.effective_permissions();
    let session_telemetry = parent_turn_context
        .session_telemetry
        .clone()
        .with_model(model.as_str(), model_info.slug.as_str());
    let reasoning_effort = per_turn_config.model_reasoning_effort;
    let reasoning_summary = per_turn_config
        .model_reasoning_summary
        .unwrap_or(model_info.default_reasoning_summary);
    let per_turn_config = Arc::new(per_turn_config);
    let turn_metadata_state = Arc::new(TurnMetadataState::new(
        session.conversation_id.to_string(),
        review_turn_id.clone(),
        parent_turn_context.cwd.to_path_buf(),
        permissions.sandbox_policy.get(),
        permissions.windows_sandbox_level,
    ));

    Arc::new(TurnContext {
        sub_id: review_turn_id,
        trace_id: current_span_trace_id(),
        realtime_active: parent_turn_context.realtime_active,
        config: per_turn_config,
        auth_manager: parent_turn_context.auth_manager.clone(),
        model_info: model_info.clone(),
        session_telemetry,
        provider: parent_turn_context.provider.clone(),
        reasoning_effort,
        reasoning_summary,
        session_source: parent_turn_context.session_source.clone(),
        environment: Arc::clone(&parent_turn_context.environment),
        tools_config,
        features: parent_turn_context.features.clone(),
        ghost_snapshot: parent_turn_context.ghost_snapshot.clone(),
        current_date: parent_turn_context.current_date.clone(),
        timezone: parent_turn_context.timezone.clone(),
        app_gateway_client_name: parent_turn_context.app_gateway_client_name.clone(),
        developer_instructions: None,
        user_instructions: None,
        compact_prompt: parent_turn_context.compact_prompt.clone(),
        collaboration_mode: parent_turn_context.collaboration_mode.clone(),
        personality: parent_turn_context.personality,
        effective_permissions: parent_turn_context.effective_permissions.clone(),
        network: parent_turn_context.network.clone(),
        shell_environment_policy: parent_turn_context.shell_environment_policy.clone(),
        cwd: parent_turn_context.cwd.clone(),
        final_output_json_schema: None,
        praxis_self_exe: parent_turn_context.praxis_self_exe.clone(),
        praxis_linux_sandbox_exe: parent_turn_context.praxis_linux_sandbox_exe.clone(),
        tool_call_gate: Arc::new(ReadinessFlag::new()),
        tool_loop_guard: Arc::new(ToolLoopGuardState::default()),
        dynamic_tools: parent_turn_context.dynamic_tools.clone(),
        truncation_policy: model_info.truncation_policy.into(),
        turn_metadata_state,
        turn_skills: TurnSkillsContext::new(parent_turn_context.turn_skills.outcome.clone()),
        turn_timing_state: Arc::new(TurnTimingState::default()),
    })
}

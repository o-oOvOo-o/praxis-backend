use std::path::PathBuf;
use std::sync::Arc;

use praxis_exec_server::Environment;
use praxis_login::AuthManager;
use praxis_network_proxy::NetworkProxy;
use praxis_otel::SessionTelemetry;
use praxis_otel::current_span_trace_id;
use praxis_protocol::ThreadId;
use praxis_protocol::openai_models::ModelInfo;
use praxis_utils_readiness::ReadinessFlag;

use crate::ModelProviderInfo;
use crate::SkillLoadOutcome;
use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::models_manager::manager::ModelsManager;
use crate::shell;
use crate::tools::loop_guard::ToolLoopGuardState;
use crate::turn_metadata::TurnMetadataState;
use crate::turn_timing::TurnTimingState;

use super::super::LiveEffectivePermissions;
use super::super::Session;
use super::super::SessionConfiguration;
use super::super::TurnSkillsContext;
use super::super::local_time_context;
use super::TurnContext;
use super::tools_config;

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::praxis) fn make_turn_context(
        conversation_id: ThreadId,
        auth_manager: Option<Arc<AuthManager>>,
        session_telemetry: &SessionTelemetry,
        provider: ModelProviderInfo,
        session_configuration: &SessionConfiguration,
        user_shell: &shell::Shell,
        shell_zsh_path: Option<&PathBuf>,
        main_execve_wrapper_exe: Option<&PathBuf>,
        per_turn_config: Config,
        model_info: ModelInfo,
        models_manager: &ModelsManager,
        llm_runtime_catalog: &LlmRuntimeCatalog,
        network: Option<NetworkProxy>,
        environment: Arc<Environment>,
        sub_id: String,
        effective_permissions: LiveEffectivePermissions,
        skills_outcome: Arc<SkillLoadOutcome>,
    ) -> TurnContext {
        let reasoning_effort = session_configuration.collaboration_mode.reasoning_effort();
        let reasoning_summary = session_configuration
            .model_reasoning_summary
            .unwrap_or(model_info.default_reasoning_summary);
        let session_telemetry = session_telemetry.clone().with_model(
            session_configuration.collaboration_mode.model(),
            model_info.slug.as_str(),
        );
        let session_source = session_configuration.session_source.clone();
        let auth_manager_for_context = auth_manager;
        let provider_for_context = provider;
        let session_telemetry_for_context = session_telemetry;
        let tools_config = tools_config::build(tools_config::TurnToolsConfigInput {
            model_info: &model_info,
            provider: &provider_for_context,
            session_configuration,
            per_turn_config: &per_turn_config,
            models_manager,
            llm_runtime_catalog,
            user_shell,
            shell_zsh_path,
            main_execve_wrapper_exe,
            reasoning_effort: reasoning_effort.as_ref(),
        });

        let cwd = session_configuration.cwd.clone();

        let per_turn_config = Arc::new(per_turn_config);
        let turn_metadata_state = Arc::new(TurnMetadataState::new(
            conversation_id.to_string(),
            sub_id.clone(),
            cwd.to_path_buf(),
            session_configuration.sandbox_policy.get(),
            session_configuration.windows_sandbox_level,
        ));
        let (current_date, timezone) = local_time_context();
        TurnContext {
            sub_id,
            trace_id: current_span_trace_id(),
            realtime_active: false,
            config: per_turn_config.clone(),
            auth_manager: auth_manager_for_context,
            model_info: model_info.clone(),
            session_telemetry: session_telemetry_for_context,
            provider: provider_for_context,
            reasoning_effort,
            reasoning_summary,
            session_source,
            environment,
            cwd,
            current_date: Some(current_date),
            timezone: Some(timezone),
            app_gateway_client_name: session_configuration.app_gateway_client_name.clone(),
            developer_instructions: session_configuration.developer_instructions.clone(),
            compact_prompt: session_configuration.compact_prompt.clone(),
            user_instructions: session_configuration.user_instructions.clone(),
            collaboration_mode: session_configuration.collaboration_mode.clone(),
            personality: session_configuration.personality,
            effective_permissions,
            network,
            shell_environment_policy: per_turn_config.permissions.shell_environment_policy.clone(),
            tools_config,
            features: per_turn_config.features.clone(),
            ghost_snapshot: per_turn_config.ghost_snapshot.clone(),
            final_output_json_schema: None,
            praxis_self_exe: per_turn_config.praxis_self_exe.clone(),
            praxis_linux_sandbox_exe: per_turn_config.praxis_linux_sandbox_exe.clone(),
            tool_call_gate: Arc::new(ReadinessFlag::new()),
            tool_loop_guard: Arc::new(ToolLoopGuardState::default()),
            truncation_policy: model_info.truncation_policy.into(),
            dynamic_tools: session_configuration.dynamic_tools.clone(),
            turn_metadata_state,
            turn_skills: TurnSkillsContext::new(skills_outcome),
            turn_timing_state: Arc::new(TurnTimingState::default()),
        }
    }
}

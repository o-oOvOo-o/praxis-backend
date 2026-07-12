use std::sync::Arc;

use crate::models_manager::manager::ModelsManager;
use praxis_utils_readiness::ReadinessFlag;

use super::TurnContext;

mod reasoning;
mod tools;

impl TurnContext {
    pub(crate) async fn with_model(&self, model: String, models_manager: &ModelsManager) -> Self {
        let mut config = (*self.config).clone();
        config.model = Some(model.clone());
        let model_info = models_manager.get_model_info(model.as_str(), &config).await;
        let truncation_policy = model_info.truncation_policy.into();
        let reasoning_effort = reasoning::resolve(self.reasoning_effort.clone(), &model_info);
        config.model_reasoning_effort = reasoning_effort.clone();

        let collaboration_mode = self.collaboration_mode.with_updates(
            Some(model.clone()),
            Some(reasoning_effort.clone()),
            /*developer_instructions*/ None,
        );
        let features = self.features.clone();
        let tools_config =
            tools::rebuild_for_model(self, models_manager, &config, &model_info, &features).await;

        Self {
            sub_id: self.sub_id.clone(),
            trace_id: self.trace_id.clone(),
            realtime_active: self.realtime_active,
            config: Arc::new(config),
            auth_manager: self.auth_manager.clone(),
            model_info: model_info.clone(),
            session_telemetry: self
                .session_telemetry
                .clone()
                .with_model(model.as_str(), model_info.slug.as_str()),
            provider: self.provider.clone(),
            reasoning_effort: reasoning_effort.clone(),
            reasoning_summary: self.reasoning_summary,
            session_source: self.session_source.clone(),
            environment: Arc::clone(&self.environment),
            cwd: self.cwd.clone(),
            current_date: self.current_date.clone(),
            timezone: self.timezone.clone(),
            app_gateway_client_name: self.app_gateway_client_name.clone(),
            developer_instructions: self.developer_instructions.clone(),
            compact_prompt: self.compact_prompt.clone(),
            user_instructions: self.user_instructions.clone(),
            collaboration_mode,
            personality: self.personality,
            effective_permissions: self.effective_permissions.clone(),
            network: self.network.clone(),
            shell_environment_policy: self.shell_environment_policy.clone(),
            tools_config,
            features,
            ghost_snapshot: self.ghost_snapshot.clone(),
            final_output_json_schema: self.final_output_json_schema.clone(),
            praxis_self_exe: self.praxis_self_exe.clone(),
            praxis_linux_sandbox_exe: self.praxis_linux_sandbox_exe.clone(),
            tool_call_gate: Arc::new(ReadinessFlag::new()),
            tool_loop_guard: Arc::clone(&self.tool_loop_guard),
            truncation_policy,
            dynamic_tools: self.dynamic_tools.clone(),
            turn_metadata_state: self.turn_metadata_state.clone(),
            turn_skills: self.turn_skills.clone(),
            turn_timing_state: Arc::clone(&self.turn_timing_state),
        }
    }
}

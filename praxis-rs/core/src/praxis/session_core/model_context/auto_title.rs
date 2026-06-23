use crate::auto_title_profile::select_auto_title_model;
use crate::llm::prompts::LlmPromptPurpose;
use crate::praxis::AutoTitleModelContext;
use crate::praxis::Session;

use super::base::SessionModelRuntimeContext;

impl Session {
    /// Returns the current model runtime context for internal metadata requests.
    pub(crate) async fn auto_title_model_context(&self) -> AutoTitleModelContext {
        let base = SessionModelRuntimeContext::capture(self).await;
        let mut selection = select_auto_title_model(
            &base.current_model_info,
            base.per_turn_config.model_provider_id.as_str(),
            &base.per_turn_config.model_provider,
        );
        if let Some(auto_title_policy) = self.llm_runtime_catalog.auto_title_task_policy_for_model(
            &base.current_model_info,
            base.per_turn_config.model_provider_id.as_str(),
            &base.per_turn_config.model_provider,
            base.product_profile,
        ) {
            if let Some(model_slug) = auto_title_policy.model_slug {
                selection.model_slug = model_slug;
            }
            if let Some(reasoning_effort) = auto_title_policy.reasoning_effort {
                selection.reasoning_effort = Some(reasoning_effort);
            }
            if let Some(suppress_model_default_reasoning) =
                auto_title_policy.suppress_model_default_reasoning
            {
                selection.suppress_model_default_reasoning = suppress_model_default_reasoning;
            }
        }

        let instructions = base.resolve_prompt(self, LlmPromptPurpose::AutoTitle);
        let mut title_model_info = if selection.model_slug == base.current_model_info.slug {
            base.current_model_info
        } else {
            self.services
                .models_manager
                .get_model_info(selection.model_slug.as_str(), &base.per_turn_config)
                .await
        };
        if selection.suppress_model_default_reasoning {
            title_model_info.default_reasoning_level = None;
        }
        AutoTitleModelContext {
            provider_id: base.per_turn_config.model_provider_id.clone(),
            provider: base.per_turn_config.model_provider.clone(),
            model_info: title_model_info.clone(),
            instructions,
            session_telemetry: self.services.session_telemetry.clone().with_model(
                selection.model_slug.as_str(),
                title_model_info.slug.as_str(),
            ),
            service_tier: base.session_configuration.service_tier,
            personality: base.session_configuration.personality,
            profile: selection.profile,
            reasoning_effort: selection.reasoning_effort,
        }
    }
}

use crate::auto_title_profile::select_auto_title_model;
use crate::llm::prompts::LlmPromptPurpose;

use super::super::AutoSummaryModelContext;
use super::super::AutoTitleModelContext;
use super::super::Session;

impl Session {
    /// Returns the current model runtime context for internal metadata requests.
    pub(crate) async fn auto_title_model_context(&self) -> AutoTitleModelContext {
        let session_configuration = {
            let state = self.state.lock().await;
            state.session_configuration.clone()
        };
        let per_turn_config = Self::build_per_turn_config(&session_configuration);
        let product_profile = session_configuration
            .session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product);
        let current_model_slug = session_configuration.collaboration_mode.model().to_string();
        let current_model_info = self
            .services
            .models_manager
            .get_model_info(current_model_slug.as_str(), &per_turn_config)
            .await;
        let mut selection = select_auto_title_model(
            &current_model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
        );
        if let Some(auto_title_policy) = self.llm_runtime_catalog.auto_title_task_policy_for_model(
            &current_model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
            product_profile,
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
        let instructions = self.llm_runtime_catalog.resolve_prompt_for_model(
            &current_model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
            product_profile,
            LlmPromptPurpose::AutoTitle,
        );
        let mut title_model_info = if selection.model_slug == current_model_info.slug {
            current_model_info
        } else {
            self.services
                .models_manager
                .get_model_info(selection.model_slug.as_str(), &per_turn_config)
                .await
        };
        if selection.suppress_model_default_reasoning {
            title_model_info.default_reasoning_level = None;
        }
        AutoTitleModelContext {
            provider_id: per_turn_config.model_provider_id.clone(),
            provider: per_turn_config.model_provider.clone(),
            model_info: title_model_info.clone(),
            instructions,
            session_telemetry: self.services.session_telemetry.clone().with_model(
                selection.model_slug.as_str(),
                title_model_info.slug.as_str(),
            ),
            service_tier: session_configuration.service_tier,
            personality: session_configuration.personality,
            profile: selection.profile,
            reasoning_effort: selection.reasoning_effort,
        }
    }

    /// Returns the current model runtime context for automatic thread summaries.
    pub(crate) async fn auto_summary_model_context(&self) -> AutoSummaryModelContext {
        let session_configuration = {
            let state = self.state.lock().await;
            state.session_configuration.clone()
        };
        let per_turn_config = Self::build_per_turn_config(&session_configuration);
        let product_profile = session_configuration
            .session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product);
        let current_model_slug = session_configuration.collaboration_mode.model().to_string();
        let model_info = self
            .services
            .models_manager
            .get_model_info(current_model_slug.as_str(), &per_turn_config)
            .await;
        let instructions = self.llm_runtime_catalog.resolve_prompt_for_model(
            &model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
            product_profile,
            LlmPromptPurpose::AutoSummary,
        );
        AutoSummaryModelContext {
            provider_id: per_turn_config.model_provider_id.clone(),
            provider: per_turn_config.model_provider.clone(),
            model_info: model_info.clone(),
            instructions,
            session_telemetry: self
                .services
                .session_telemetry
                .clone()
                .with_model(current_model_slug.as_str(), model_info.slug.as_str()),
            service_tier: session_configuration.service_tier,
            personality: session_configuration.personality,
        }
    }
}

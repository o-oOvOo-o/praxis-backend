use praxis_protocol::openai_models::ModelInfo;

use crate::config::Config;
use crate::llm::ids::ProductProfileId;
use crate::llm::prompts::LlmPromptPurpose;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;

pub(super) struct SessionModelRuntimeContext {
    pub(super) session_configuration: SessionConfiguration,
    pub(super) per_turn_config: Config,
    pub(super) product_profile: Option<ProductProfileId>,
    pub(super) current_model_slug: String,
    pub(super) current_model_info: ModelInfo,
}

impl SessionModelRuntimeContext {
    pub(super) async fn capture(session: &Session) -> Self {
        let session_configuration = {
            let state = session.state.lock().await;
            state.session_configuration.clone()
        };
        let per_turn_config = Session::build_per_turn_config(&session_configuration);
        let product_profile = session_configuration
            .session_source
            .restriction_product()
            .and_then(ProductProfileId::from_product);
        let current_model_slug = session_configuration.collaboration_mode.model().to_string();
        let current_model_info = session
            .services
            .models_manager
            .get_model_info(current_model_slug.as_str(), &per_turn_config)
            .await;
        Self {
            session_configuration,
            per_turn_config,
            product_profile,
            current_model_slug,
            current_model_info,
        }
    }

    pub(super) fn resolve_prompt(
        &self,
        session: &Session,
        purpose: LlmPromptPurpose,
    ) -> Option<String> {
        session.llm_runtime_catalog.resolve_prompt_for_model(
            &self.current_model_info,
            self.per_turn_config.model_provider_id.as_str(),
            &self.per_turn_config.model_provider,
            self.product_profile.clone(),
            purpose,
        )
    }
}

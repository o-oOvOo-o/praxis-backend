use super::super::Session;
use super::super::SessionConfiguration;
use super::super::SessionSettingsUpdate;

impl Session {
    pub(in crate::praxis) fn prompt_route_update_needed(
        previous: &SessionConfiguration,
        next: &SessionConfiguration,
        updates: &SessionSettingsUpdate,
    ) -> bool {
        updates.model_provider.is_some()
            || updates.personality.is_some()
            || updates.collaboration_mode.as_ref().is_some_and(|_| {
                previous.collaboration_mode.model() != next.collaboration_mode.model()
            })
    }

    pub(in crate::praxis) async fn refresh_model_base_instructions(
        &self,
        session_configuration: &mut SessionConfiguration,
    ) {
        let per_turn_config = Self::build_per_turn_config(session_configuration);
        if let Some(base_instructions) = per_turn_config.base_instructions.clone() {
            session_configuration.base_instructions = base_instructions;
            return;
        }

        let model = session_configuration.collaboration_mode.model().to_string();
        let model_info = self
            .services
            .models_manager
            .get_model_info(model.as_str(), &per_turn_config)
            .await;
        let product_profile = session_configuration
            .session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product);
        session_configuration.base_instructions =
            crate::prompt_profiles::resolve_model_instructions(
                &model_info,
                per_turn_config.model_provider_id.as_str(),
                &per_turn_config.model_provider,
                session_configuration.personality,
                product_profile,
                &self.llm_runtime_catalog,
            );
    }
}

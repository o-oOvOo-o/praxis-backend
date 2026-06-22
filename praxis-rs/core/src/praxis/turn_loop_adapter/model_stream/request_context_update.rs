use std::sync::Arc;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::request_settings::PraxisRoundSettings;

pub(super) fn round_settings_change_context(
    turn_context: &TurnContext,
    settings: &PraxisRoundSettings,
) -> bool {
    settings.model_slug != turn_context.model_info.slug
        || settings.reasoning.is_some() && settings.reasoning != turn_context.reasoning_effort
        || settings.service_tier.is_some()
            && settings.service_tier != turn_context.config.service_tier
}

pub(super) async fn apply_round_settings(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    settings: PraxisRoundSettings,
) -> TurnContext {
    let mut effective_context = turn_context
        .with_model(settings.model_slug, &session.services.models_manager)
        .await;
    let mut effective_config = (*effective_context.config).clone();
    let effective_reasoning = settings.reasoning.or(effective_context.reasoning_effort);
    let effective_service_tier = settings.service_tier.or(effective_config.service_tier);
    effective_config.model_reasoning_effort = effective_reasoning;
    effective_config.service_tier = effective_service_tier;
    effective_context.config = Arc::new(effective_config);
    effective_context.reasoning_effort = effective_reasoning;
    effective_context.collaboration_mode = effective_context.collaboration_mode.with_updates(
        Some(effective_context.model_info.slug.clone()),
        Some(effective_reasoning),
        None,
    );

    effective_context
}

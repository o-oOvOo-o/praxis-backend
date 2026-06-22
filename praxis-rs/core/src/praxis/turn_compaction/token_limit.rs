use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(in crate::praxis) fn effective_auto_compact_token_limit(
    sess: &Session,
    turn_context: &TurnContext,
) -> Option<i64> {
    let model_limit: Option<i64> = turn_context.model_info.auto_compact_token_limit();
    let product_profile = turn_context
        .session_source
        .restriction_product()
        .and_then(crate::llm::ids::ProductProfileId::from_product);
    let profile_cap: Option<i64> = sess
        .llm_runtime_catalog()
        .auto_compact_token_limit_cap_for_model(
            &turn_context.model_info,
            &turn_context.config.model_provider_id,
            &turn_context.provider,
            product_profile,
        )
        .filter(|cap| *cap > 0);

    match (model_limit, profile_cap) {
        (Some(model_limit), Some(profile_cap)) => Some(model_limit.min(profile_cap)),
        (Some(model_limit), None) => Some(model_limit),
        (None, Some(profile_cap)) => Some(profile_cap),
        (None, None) => None,
    }
}

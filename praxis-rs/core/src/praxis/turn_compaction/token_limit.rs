use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(in crate::praxis) async fn effective_auto_compact_token_limit(
    sess: &Session,
    turn_context: &TurnContext,
) -> Option<i64> {
    let model_limit: Option<i64> = turn_context.model_info.auto_compact_token_limit();
    let governance_limit = sess
        .context_governance
        .compact_threshold(turn_context)
        .await;
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

    [model_limit, governance_limit, profile_cap]
        .into_iter()
        .flatten()
        .filter(|limit| *limit > 0)
        .min()
}

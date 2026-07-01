use crate::llm::ids::ProductProfileId;
use crate::llm::runtime::LlmToolVisibilityPolicy;
use crate::model_provider_info::is_native_local_provider;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) fn resolve(
    sess: &Session,
    turn_context: &TurnContext,
) -> Option<LlmToolVisibilityPolicy> {
    if is_native_local_provider(
        &turn_context.config.model_provider_id,
        &turn_context.provider,
    ) {
        return Some(LlmToolVisibilityPolicy::allow_none());
    }

    sess.llm_runtime_catalog().tool_visibility_policy_for_model(
        &turn_context.model_info,
        &turn_context.config.model_provider_id,
        &turn_context.provider,
        turn_context
            .session_source
            .restriction_product()
            .and_then(ProductProfileId::from_product),
    )
}

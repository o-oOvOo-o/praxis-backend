use super::super::plugin::ProfileMatchContext;
use super::super::plugin::ProfileProviderIdentityRule;
use crate::llm::ids::WireId;
use crate::model_provider_info::OPENAI_PROVIDER_ID;

const OPENAI_PROVIDER_NAME: &str = "OpenAI";
const OPENAI_PROVIDER_RULE: ProfileProviderIdentityRule = ProfileProviderIdentityRule::new(
    &[OPENAI_PROVIDER_ID],
    &[],
    &[OPENAI_PROVIDER_NAME],
    &[],
    &["api.openai.com"],
);

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    let model = ctx.model_info.slug.to_ascii_lowercase();
    ctx.wire_id_is(WireId::Responses)
        && (ctx.provider_identity().id_eq(OPENAI_PROVIDER_ID) || is_first_party_model(&model))
}

pub(super) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    OPENAI_PROVIDER_RULE.matches_provider(provider_id, provider)
}

pub(super) fn is_first_party_model(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.starts_with("gpt-")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("chatgpt-")
        || model.contains("-codex")
}

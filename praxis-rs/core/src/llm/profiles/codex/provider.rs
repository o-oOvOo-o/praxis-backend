use super::super::plugin::ProfileMatchContext;
use crate::model_provider_info::OPENAI_PROVIDER_ID;
use crate::model_provider_info::WireApi;

const OPENAI_PROVIDER_NAME: &str = "OpenAI";

pub(crate) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    let model = ctx.model_info.slug.to_ascii_lowercase();
    ctx.provider.wire_api == WireApi::Responses
        && (ctx.provider_id == OPENAI_PROVIDER_ID
            || model.contains("codex")
            || model.starts_with("gpt-"))
}

pub(crate) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    provider_id == OPENAI_PROVIDER_ID
        || provider.name.eq_ignore_ascii_case(OPENAI_PROVIDER_NAME)
        || provider
            .base_url
            .as_deref()
            .is_some_and(|base_url| base_url.to_ascii_lowercase().contains("api.openai.com"))
}

pub(crate) fn is_first_party_model(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.starts_with("gpt-")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("chatgpt-")
        || model.contains("-codex")
}

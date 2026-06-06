use super::super::plugin::ProfileMatchContext;
use super::super::plugin::base_url;
use super::super::plugin::contains_any_text;

pub(crate) const QWEN_PROVIDER_ID: &str = "qwen";

pub(crate) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    contains_any_text(
        &[
            ctx.model_info.slug.as_str(),
            ctx.provider_id,
            ctx.provider.name.as_str(),
            base_url(ctx.provider),
        ],
        &["qwen", "dashscope"],
    )
}

pub(crate) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    let provider_name = provider.name.to_ascii_lowercase();
    provider_id.eq_ignore_ascii_case(QWEN_PROVIDER_ID)
        || provider_id.eq_ignore_ascii_case("dashscope")
        || provider_name.contains("qwen")
        || provider_name.contains("dashscope")
        || provider.base_url.as_deref().is_some_and(|base_url| {
            base_url
                .to_ascii_lowercase()
                .contains("dashscope.aliyuncs.com")
        })
}

pub(crate) fn is_first_party_model(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    if model.is_empty() || model.starts_with("qwen-image") {
        return false;
    }

    model.starts_with("qwen") || model.starts_with("qwq") || model.starts_with("qvq")
}

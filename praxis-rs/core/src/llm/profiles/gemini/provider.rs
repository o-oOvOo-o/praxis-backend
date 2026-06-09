use super::super::plugin::ProfileMatchContext;
use super::super::plugin::base_url;
use super::super::plugin::contains_any_text;

pub(crate) const GEMINI_PROVIDER_ID: &str = "gemini";

pub(crate) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    contains_any_text(
        &[
            ctx.model_info.slug.as_str(),
            ctx.provider_id,
            ctx.provider.name.as_str(),
            base_url(ctx.provider),
        ],
        &[
            "gemini",
            "generativelanguage.googleapis.com",
            "aiplatform.googleapis.com",
        ],
    )
}

pub(crate) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    let provider_name = provider.name.to_ascii_lowercase();
    provider_id.eq_ignore_ascii_case(GEMINI_PROVIDER_ID)
        || provider_name.contains("gemini")
        || provider.base_url.as_deref().is_some_and(|base_url| {
            let base_url = base_url.to_ascii_lowercase();
            base_url.contains("generativelanguage.googleapis.com")
                || base_url.contains("aiplatform.googleapis.com")
        })
}

pub(crate) fn is_first_party_model(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("gemini-")
}

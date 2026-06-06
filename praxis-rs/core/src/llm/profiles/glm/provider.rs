use super::super::plugin::ProfileMatchContext;
use super::super::plugin::base_url;
use super::super::plugin::contains_any_text;

pub(crate) const GLM_PROVIDER_ID: &str = "glm";

pub(crate) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    contains_any_text(
        &[
            ctx.model_info.slug.as_str(),
            ctx.provider_id,
            ctx.provider.name.as_str(),
            base_url(ctx.provider),
        ],
        &["glm", "bigmodel", "z.ai", "zai"],
    )
}

pub(crate) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    provider_id.eq_ignore_ascii_case(GLM_PROVIDER_ID)
        || provider.name.to_ascii_lowercase().contains("glm")
        || provider.base_url.as_deref().is_some_and(|base_url| {
            let base_url = base_url.to_ascii_lowercase();
            base_url.contains("bigmodel.cn") || base_url.contains("z.ai")
        })
}

pub(crate) fn is_first_party_model(model: &str) -> bool {
    matches!(
        model.trim().to_ascii_lowercase().as_str(),
        "glm-5.1" | "glm-5"
    )
}

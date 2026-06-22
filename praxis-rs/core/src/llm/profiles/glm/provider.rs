use super::super::plugin::ProfileMatchContext;
use super::super::plugin::ProfileProviderIdentityRule;

pub(super) const GLM_PROVIDER_ID: &str = "glm";
const GLM_CONTEXT_NEEDLES: &[&str] = &["glm", "bigmodel", "z.ai", "zai"];
const GLM_BASE_URL_NEEDLES: &[&str] = &["bigmodel.cn", "z.ai"];
const GLM_PROVIDER_RULE: ProfileProviderIdentityRule =
    ProfileProviderIdentityRule::new(&[], &[GLM_PROVIDER_ID], &[], &["glm"], GLM_BASE_URL_NEEDLES);

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.model_and_provider_identity_contains_any(GLM_CONTEXT_NEEDLES)
}

pub(super) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    GLM_PROVIDER_RULE.matches_provider(provider_id, provider)
}

pub(super) fn is_first_party_model(model: &str) -> bool {
    matches!(
        model.trim().to_ascii_lowercase().as_str(),
        "glm-5.1" | "glm-5"
    )
}

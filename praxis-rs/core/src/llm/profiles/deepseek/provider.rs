use super::super::plugin::ProfileMatchContext;
use super::super::plugin::ProfileProviderIdentityRule;

pub(super) const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
const DEEPSEEK_PROVIDER_RULE: ProfileProviderIdentityRule = ProfileProviderIdentityRule::new(
    &[],
    &[DEEPSEEK_PROVIDER_ID],
    &[DEEPSEEK_PROVIDER_ID],
    &[],
    &["api.deepseek.com"],
);

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.model_and_provider_identity_contains_any(&["deepseek"])
}

pub(super) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    DEEPSEEK_PROVIDER_RULE.matches_provider(provider_id, provider)
}

pub(super) fn is_first_party_model(model: &str) -> bool {
    matches!(
        model.trim().to_ascii_lowercase().as_str(),
        "deepseek-v4-pro" | "deepseek-v4-flash"
    )
}

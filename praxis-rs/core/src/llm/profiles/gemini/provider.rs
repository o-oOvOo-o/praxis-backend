use super::super::plugin::ProfileMatchContext;
use super::super::plugin::ProfileProviderIdentityRule;

pub(super) const GEMINI_PROVIDER_ID: &str = "gemini";
const GEMINI_CONTEXT_NEEDLES: &[&str] = &[
    "gemini",
    "generativelanguage.googleapis.com",
    "aiplatform.googleapis.com",
];
const GEMINI_BASE_URL_NEEDLES: &[&str] = &[
    "generativelanguage.googleapis.com",
    "aiplatform.googleapis.com",
];
const GEMINI_PROVIDER_RULE: ProfileProviderIdentityRule = ProfileProviderIdentityRule::new(
    &[],
    &[GEMINI_PROVIDER_ID],
    &[],
    &["gemini"],
    GEMINI_BASE_URL_NEEDLES,
);

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.model_and_provider_identity_contains_any(GEMINI_CONTEXT_NEEDLES)
}

pub(super) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    GEMINI_PROVIDER_RULE.matches_provider(provider_id, provider)
}

pub(super) fn is_first_party_model(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("gemini-")
}

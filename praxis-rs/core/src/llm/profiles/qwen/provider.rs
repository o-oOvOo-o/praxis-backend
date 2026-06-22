use super::super::plugin::ProfileMatchContext;
use super::super::plugin::ProfileProviderIdentityRule;

pub(super) const QWEN_PROVIDER_ID: &str = "qwen";
const QWEN_CONTEXT_NEEDLES: &[&str] = &["qwen", "dashscope"];
const QWEN_PROVIDER_NAME_NEEDLES: &[&str] = &["qwen", "dashscope"];
const QWEN_BASE_URL_NEEDLES: &[&str] = &["dashscope.aliyuncs.com"];
const QWEN_PROVIDER_RULE: ProfileProviderIdentityRule = ProfileProviderIdentityRule::new(
    &[],
    &[QWEN_PROVIDER_ID, "dashscope"],
    &[],
    QWEN_PROVIDER_NAME_NEEDLES,
    QWEN_BASE_URL_NEEDLES,
);

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.model_and_provider_identity_contains_any(QWEN_CONTEXT_NEEDLES)
}

pub(super) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    QWEN_PROVIDER_RULE.matches_provider(provider_id, provider)
}

pub(super) fn is_first_party_model(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    if model.is_empty() || model.starts_with("qwen-image") {
        return false;
    }

    model.starts_with("qwen") || model.starts_with("qwq") || model.starts_with("qvq")
}

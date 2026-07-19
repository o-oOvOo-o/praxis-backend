use super::super::plugin::ProfileMatchContext;
use crate::llm::provider::KIMI_PROVIDER_ID;
use crate::llm::provider::ModelProviderInfo;

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.model_and_provider_identity_contains_any(&["kimi", "api.kimi.com", "k3"])
}

pub(super) fn is_first_party_provider(provider_id: &str, provider: &ModelProviderInfo) -> bool {
    provider_id == KIMI_PROVIDER_ID || provider.is_kimi()
}

pub(super) fn is_first_party_model(model: &str) -> bool {
    matches!(
        model.trim().to_ascii_lowercase().as_str(),
        "k3" | "k3[1m]" | "kimi-for-coding" | "kimi-for-coding-highspeed"
    )
}

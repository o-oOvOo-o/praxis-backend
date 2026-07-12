use super::super::plugin::ProfileMatchContext;
use crate::llm::ids::WireId;
use crate::model_provider_info::ANTHROPIC_PROVIDER_ID;
use crate::model_provider_info::ModelProviderInfo;

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.wire_id_is(WireId::ClaudeMessages)
        || ctx.model_and_provider_identity_contains_any(&["anthropic", "claude"])
}

pub(super) fn is_fable_5(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.model_info
        .slug
        .trim()
        .eq_ignore_ascii_case("claude-fable-5")
}

pub(super) fn is_first_party_provider(provider_id: &str, provider: &ModelProviderInfo) -> bool {
    provider_id == ANTHROPIC_PROVIDER_ID || provider.is_anthropic()
}

pub(super) fn is_first_party_model(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("claude-")
}

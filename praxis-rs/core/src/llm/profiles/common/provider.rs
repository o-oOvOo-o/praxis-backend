use super::super::plugin::ProfileMatchContext;
use crate::llm::ids::WireId;
use crate::model_provider_info::ModelProviderInfo;

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.wire_id_is(WireId::OpenAiCompat)
}

pub(super) fn is_generic_provider(_provider_id: &str, provider: &ModelProviderInfo) -> bool {
    WireId::from(provider.wire_api) == WireId::OpenAiCompat
}

pub(super) fn is_generic_model(model: &str) -> bool {
    !model.trim().is_empty()
}

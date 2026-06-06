use super::super::plugin::ProfileMatchContext;
use crate::model_provider_info::WireApi;

pub(crate) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.provider.wire_api == WireApi::OpenAiCompat
}

pub(crate) fn is_generic_provider(_provider_id: &str, provider: &crate::ModelProviderInfo) -> bool {
    provider.wire_api == WireApi::OpenAiCompat
}

pub(crate) fn is_generic_model(model: &str) -> bool {
    !model.trim().is_empty()
}

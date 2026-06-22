use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::protocol::SessionSource;
use praxis_tools::ToolCapabilityConfig;
use praxis_tools::ToolWireProfile;

use crate::ModelProviderInfo;
use crate::WireApi;
use crate::llm::runtime::LlmRuntimeCatalog;

pub(super) fn tool_wire_profile_for_wire_api(wire_api: WireApi) -> ToolWireProfile {
    match wire_api {
        WireApi::Responses => ToolWireProfile::Responses,
        WireApi::Claude => ToolWireProfile::Claude,
        WireApi::OpenAiCompat => ToolWireProfile::Common,
    }
}

pub(super) fn tool_capabilities_for_turn_model(
    llm_runtime_catalog: &LlmRuntimeCatalog,
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
    session_source: &SessionSource,
) -> ToolCapabilityConfig {
    llm_runtime_catalog.tool_capabilities_for_model(
        model_info,
        provider_id,
        provider,
        session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product),
    )
}

use praxis_protocol::config_types::MultiAgentMode;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::MultiAgentVersion;
use praxis_protocol::openai_models::ReasoningEffort;
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

pub(super) fn multi_agent_mode_for_turn_model(
    model_info: &ModelInfo,
    reasoning_effort: Option<&ReasoningEffort>,
) -> MultiAgentMode {
    match reasoning_effort {
        Some(ReasoningEffort::Ultra)
            if model_info.multi_agent_version == Some(MultiAgentVersion::V2) =>
        {
            MultiAgentMode::Proactive
        }
        _ => MultiAgentMode::ExplicitRequestOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_protocol::openai_models::known_openai_compatible_model_info;

    #[test]
    fn proactive_mode_requires_both_ultra_and_model_support() {
        let sol = known_openai_compatible_model_info("gpt-5.6-sol").unwrap();
        let luna = known_openai_compatible_model_info("gpt-5.6-luna").unwrap();

        assert_eq!(
            multi_agent_mode_for_turn_model(&sol, Some(&ReasoningEffort::Ultra)),
            MultiAgentMode::Proactive
        );
        assert_eq!(
            multi_agent_mode_for_turn_model(&sol, Some(&ReasoningEffort::Max)),
            MultiAgentMode::ExplicitRequestOnly
        );
        assert_eq!(
            multi_agent_mode_for_turn_model(&luna, Some(&ReasoningEffort::Ultra)),
            MultiAgentMode::ExplicitRequestOnly
        );
    }
}

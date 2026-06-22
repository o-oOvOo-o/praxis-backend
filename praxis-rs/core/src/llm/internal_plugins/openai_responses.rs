#[cfg(test)]
use crate::llm::ids::WireId;
use crate::llm::internal_plugins::LlmPlugin;
#[cfg(test)]
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::exclusive_model_catalog;
use crate::llm::profiles::openai_responses;

#[cfg(test)]
const RESPONSES_WEB_SEARCH_BACKEND_ID: &str = "web_search/responses";

pub(super) struct OpenAiResponsesLlmPlugin;

impl LlmPlugin for OpenAiResponsesLlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "openai_responses",
            label: "OpenAI Responses",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        let profile = openai_responses::profile();
        #[cfg(test)]
        registry.add_wire_descriptor(WireId::Responses, "OpenAI Responses");
        #[cfg(test)]
        registry.add_profile_provider_extension(profile, "OpenAI");
        registry.add_model_catalog(exclusive_model_catalog(
            "openai-gpt",
            "OpenAI GPT models",
            openai_responses::is_first_party_provider,
            openai_responses::is_first_party_model,
        ));
        #[cfg(test)]
        registry.add_profile_extension_bundle(
            profile,
            ("openai_responses/prompts", "OpenAI Responses prompt layer"),
            ("openai_responses/tasks", "OpenAI Responses task policy"),
            ("openai_responses/tools", "OpenAI Responses tool dialect"),
        );
        #[cfg(test)]
        registry
            .add_tool_backend_extension(RESPONSES_WEB_SEARCH_BACKEND_ID, "Responses web_search");
        registry.add_profile(profile);
    }
}

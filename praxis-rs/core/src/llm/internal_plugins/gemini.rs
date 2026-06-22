use crate::llm::internal_plugins::LlmPlugin;
#[cfg(test)]
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::provider_model_catalog;
use crate::llm::profiles::gemini;

pub(super) struct GeminiLlmPlugin;

impl LlmPlugin for GeminiLlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "gemini",
            label: "Gemini",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        let profile = gemini::profile();
        #[cfg(test)]
        registry.add_openai_compat_wire();
        #[cfg(test)]
        registry.add_profile_provider_extension(profile, "Gemini");
        registry.add_model_catalog(provider_model_catalog(
            "gemini-models",
            "Gemini models",
            gemini::is_first_party_provider,
            gemini::is_first_party_model,
        ));
        #[cfg(test)]
        registry.add_profile_extension_bundle(
            profile,
            ("gemini/prompts", "Gemini prompt layer"),
            ("gemini/tasks", "Gemini task policy"),
            ("gemini/tools", "Gemini tool dialect"),
        );
        registry.add_profile(profile);
    }
}

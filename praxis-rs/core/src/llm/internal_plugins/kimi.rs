use crate::llm::internal_plugins::LlmPlugin;
#[cfg(test)]
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::exclusive_model_catalog;
use crate::llm::profiles::kimi;

pub(super) struct KimiLlmPlugin;

impl LlmPlugin for KimiLlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "kimi",
            label: "Kimi",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        let profile = kimi::profile();
        registry.add_model_catalog(exclusive_model_catalog(
            "kimi-models",
            "Kimi Code models",
            kimi::is_first_party_provider,
            kimi::is_first_party_model,
        ));
        #[cfg(test)]
        registry.add_profile_extension_bundle(
            &profile,
            ("kimi/prompts", "Kimi prompt layer"),
            ("kimi/tasks", "Kimi task policy"),
            ("kimi/tools", "Kimi tool dialect"),
        );
        registry.add_profile(profile);
    }
}

use crate::llm::internal_plugins::LlmPlugin;
#[cfg(test)]
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::provider_model_catalog;
use crate::llm::profiles::qwen;

pub(super) struct QwenLlmPlugin;

impl LlmPlugin for QwenLlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "qwen",
            label: "Qwen",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        let profile = qwen::profile();
        #[cfg(test)]
        registry.add_openai_compat_wire();
        #[cfg(test)]
        registry.add_profile_provider_extension(profile, "Qwen");
        registry.add_model_catalog(provider_model_catalog(
            "qwen-models",
            "Qwen models",
            qwen::is_first_party_provider,
            qwen::is_first_party_model,
        ));
        #[cfg(test)]
        registry.add_profile_extension_bundle(
            profile,
            ("qwen/prompts", "Qwen prompt layer"),
            ("qwen/tasks", "Qwen task policy"),
            ("qwen/tools", "Qwen tool dialect"),
        );
        registry.add_profile(profile);
    }
}

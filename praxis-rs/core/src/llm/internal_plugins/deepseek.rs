use crate::llm::internal_plugins::LlmPlugin;
#[cfg(test)]
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::exclusive_model_catalog;
use crate::llm::profiles::deepseek;

pub(super) struct DeepSeekLlmPlugin;

impl LlmPlugin for DeepSeekLlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "deepseek",
            label: "DeepSeek",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        let profile = deepseek::profile();
        #[cfg(test)]
        registry.add_openai_compat_wire();
        #[cfg(test)]
        registry.add_profile_provider_extension(profile, "DeepSeek");
        registry.add_model_catalog(exclusive_model_catalog(
            "deepseek-models",
            "DeepSeek models",
            deepseek::is_first_party_provider,
            deepseek::is_first_party_model,
        ));
        #[cfg(test)]
        registry.add_profile_extension_bundle(
            profile,
            ("deepseek/prompts", "DeepSeek prompt layer"),
            ("deepseek/tasks", "DeepSeek task policy"),
            ("deepseek/tools", "DeepSeek tool dialect"),
        );
        #[cfg(test)]
        registry.add_prompt_layer_extension(
            "deepseek/smarter",
            "DeepSeek smarter orchestration prompt layer",
        );
        registry.add_profile(profile);
    }
}

use crate::llm::internal_plugins::LlmPlugin;
#[cfg(test)]
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::generic_model_catalog;
use crate::llm::profiles::common;

pub(super) struct CommonOpenAiCompatLlmPlugin;

impl LlmPlugin for CommonOpenAiCompatLlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "openai_compat",
            label: "OpenAI-compatible",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        let profile = common::profile();
        #[cfg(test)]
        registry.add_openai_compat_wire();
        registry.add_model_catalog(generic_model_catalog(
            "openai_compat/model_catalog",
            "OpenAI-compatible model catalog",
            common::is_generic_provider,
            common::is_generic_model,
        ));
        #[cfg(test)]
        registry.add_profile_extension_bundle(
            profile,
            ("openai_compat/prompts", "OpenAI-compatible prompt layer"),
            ("openai_compat/tasks", "OpenAI-compatible task policy"),
            ("openai_compat/tools", "OpenAI-compatible tool dialect"),
        );
        registry.add_profile(profile);
        super::common_branches::register_common_branches(registry);
    }
}

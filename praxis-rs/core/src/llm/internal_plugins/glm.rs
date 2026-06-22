use crate::llm::internal_plugins::LlmPlugin;
#[cfg(test)]
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::provider_model_catalog;
use crate::llm::profiles::glm;

pub(super) struct GlmLlmPlugin;

impl LlmPlugin for GlmLlmPlugin {
    #[cfg(test)]
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "glm",
            label: "GLM",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        let profile = glm::profile();
        #[cfg(test)]
        registry.add_openai_compat_wire();
        #[cfg(test)]
        registry.add_profile_provider_extension(profile, "GLM");
        registry.add_model_catalog(provider_model_catalog(
            "glm-models",
            "GLM models",
            glm::is_first_party_provider,
            glm::is_first_party_model,
        ));
        #[cfg(test)]
        registry.add_profile_extension_bundle(
            profile,
            ("glm/prompts", "GLM prompt layer"),
            ("glm/tasks", "GLM task policy"),
            ("glm/tools", "GLM tool dialect"),
        );
        registry.add_profile(profile);
    }
}

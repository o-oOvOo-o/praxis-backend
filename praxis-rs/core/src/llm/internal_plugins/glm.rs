use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::WireId;
use crate::llm::internal_plugins::LlmExtensionDescriptor;
use crate::llm::internal_plugins::LlmExtensionKind;
use crate::llm::internal_plugins::LlmPlugin;
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::behavior_extension;
use crate::llm::internal_plugins::provider_model_catalog;
use crate::llm::internal_plugins::wire_extension;
use crate::llm::profiles::glm;
use crate::llm::wire::plugin::WireDescriptor;

pub(crate) struct GlmLlmPlugin;

impl LlmPlugin for GlmLlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "glm",
            label: "GLM",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        registry.add_wire(WireDescriptor {
            id: WireId::OpenAiCompat,
            name: "OpenAI-compatible Chat Completions",
        });
        registry.add_extension(wire_extension(
            WireId::OpenAiCompat,
            "OpenAI-compatible Chat Completions",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::Provider,
            glm::provider::GLM_PROVIDER_ID,
            "GLM",
        ));
        registry.add_model_catalog(provider_model_catalog(
            "glm-models",
            "GLM models",
            glm::provider::is_first_party_provider,
            glm::provider::is_first_party_model,
        ));
        registry.add_extension(behavior_extension(BehaviorProfileId::Glm, "GLM"));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::PromptLayer,
            "glm/prompts",
            "GLM prompt layer",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::TaskPolicy,
            "glm/tasks",
            "GLM task policy",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolPolicy,
            "glm/tools",
            "GLM tool dialect",
        ));
        registry.add_profile(glm::profile());
    }
}

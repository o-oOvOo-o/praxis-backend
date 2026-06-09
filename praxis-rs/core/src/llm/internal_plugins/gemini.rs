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
use crate::llm::profiles::gemini;
use crate::llm::wire::plugin::WireDescriptor;

pub(crate) struct GeminiLlmPlugin;

impl LlmPlugin for GeminiLlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "gemini",
            label: "Gemini",
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
            gemini::provider::GEMINI_PROVIDER_ID,
            "Gemini",
        ));
        registry.add_model_catalog(provider_model_catalog(
            "gemini-models",
            "Gemini models",
            gemini::provider::is_first_party_provider,
            gemini::provider::is_first_party_model,
        ));
        registry.add_extension(behavior_extension(BehaviorProfileId::Gemini, "Gemini"));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::PromptLayer,
            "gemini/prompts",
            "Gemini prompt layer",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::TaskPolicy,
            "gemini/tasks",
            "Gemini task policy",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolPolicy,
            "gemini/tools",
            "Gemini tool dialect",
        ));
        registry.add_profile(gemini::profile());
    }
}

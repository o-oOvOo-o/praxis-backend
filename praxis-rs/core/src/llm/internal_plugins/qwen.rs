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
use crate::llm::profiles::qwen;
use crate::llm::wire::plugin::WireDescriptor;

pub(crate) struct QwenLlmPlugin;

impl LlmPlugin for QwenLlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "qwen",
            label: "Qwen",
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
            qwen::provider::QWEN_PROVIDER_ID,
            "Qwen",
        ));
        registry.add_model_catalog(provider_model_catalog(
            "qwen-models",
            "Qwen models",
            qwen::provider::is_first_party_provider,
            qwen::provider::is_first_party_model,
        ));
        registry.add_extension(behavior_extension(BehaviorProfileId::Qwen, "Qwen"));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::PromptLayer,
            "qwen/prompts",
            "Qwen prompt layer",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::TaskPolicy,
            "qwen/tasks",
            "Qwen task policy",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolPolicy,
            "qwen/tools",
            "Qwen tool dialect",
        ));
        registry.add_profile(qwen::profile());
    }
}

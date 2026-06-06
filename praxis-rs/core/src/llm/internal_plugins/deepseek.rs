use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::WireId;
use crate::llm::internal_plugins::LlmExtensionDescriptor;
use crate::llm::internal_plugins::LlmExtensionKind;
use crate::llm::internal_plugins::LlmPlugin;
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::behavior_extension;
use crate::llm::internal_plugins::exclusive_model_catalog;
use crate::llm::internal_plugins::wire_extension;
use crate::llm::profiles::deepseek;
use crate::llm::wire::plugin::WireDescriptor;

pub(crate) struct DeepSeekLlmPlugin;

impl LlmPlugin for DeepSeekLlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "deepseek",
            label: "DeepSeek",
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
            deepseek::provider::DEEPSEEK_PROVIDER_ID,
            "DeepSeek",
        ));
        registry.add_model_catalog(exclusive_model_catalog(
            "deepseek-models",
            "DeepSeek models",
            deepseek::provider::is_first_party_provider,
            deepseek::provider::is_first_party_model,
        ));
        registry.add_extension(behavior_extension(BehaviorProfileId::DeepSeek, "DeepSeek"));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::PromptLayer,
            "deepseek/prompts",
            "DeepSeek prompt layer",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::TaskPolicy,
            "deepseek/tasks",
            "DeepSeek task policy",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolPolicy,
            "deepseek/tools",
            "DeepSeek tool dialect",
        ));
        registry.add_profile(deepseek::profile());
    }
}

use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::WireId;
use crate::llm::internal_plugins::LlmExtensionDescriptor;
use crate::llm::internal_plugins::LlmExtensionKind;
use crate::llm::internal_plugins::LlmPlugin;
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::behavior_extension;
use crate::llm::internal_plugins::generic_model_catalog;
use crate::llm::internal_plugins::wire_extension;
use crate::llm::profiles::common;
use crate::llm::wire::plugin::WireDescriptor;

pub(crate) struct CommonOpenAiCompatLlmPlugin;

impl LlmPlugin for CommonOpenAiCompatLlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "openai_compat",
            label: "OpenAI-compatible",
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
        registry.add_model_catalog(generic_model_catalog(
            "openai_compat/model_catalog",
            "OpenAI-compatible model catalog",
            common::provider::is_generic_provider,
            common::provider::is_generic_model,
        ));
        registry.add_extension(behavior_extension(
            BehaviorProfileId::Common,
            "OpenAI-compatible",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::PromptLayer,
            "openai_compat/prompts",
            "OpenAI-compatible prompt layer",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::TaskPolicy,
            "openai_compat/tasks",
            "OpenAI-compatible task policy",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolPolicy,
            "openai_compat/tools",
            "OpenAI-compatible tool dialect",
        ));
        registry.add_profile(common::profile());
        super::common_branches::register_common_branches(registry);
    }
}

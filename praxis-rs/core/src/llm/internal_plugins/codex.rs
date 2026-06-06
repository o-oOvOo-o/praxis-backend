use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::WireId;
use crate::llm::internal_plugins::LlmExtensionDescriptor;
use crate::llm::internal_plugins::LlmExtensionKind;
use crate::llm::internal_plugins::LlmPlugin;
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::behavior_extension;
use crate::llm::internal_plugins::exclusive_model_catalog;
use crate::llm::internal_plugins::tool_backend_extension;
use crate::llm::internal_plugins::wire_extension;
use crate::llm::profiles::codex;
use crate::llm::wire::plugin::WireDescriptor;

const RESPONSES_WEB_SEARCH_BACKEND_ID: &str = "web_search/responses";

pub(crate) struct CodexLlmPlugin;

impl LlmPlugin for CodexLlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "codex",
            label: "Codex",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        registry.add_wire(WireDescriptor {
            id: WireId::Responses,
            name: "OpenAI Responses",
        });
        registry.add_extension(wire_extension(WireId::Responses, "OpenAI Responses"));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::Provider,
            "openai",
            "OpenAI",
        ));
        registry.add_model_catalog(exclusive_model_catalog(
            "openai-gpt",
            "OpenAI GPT and Codex models",
            codex::provider::is_first_party_provider,
            codex::provider::is_first_party_model,
        ));
        registry.add_extension(behavior_extension(
            BehaviorProfileId::CodexResponses,
            "Codex Responses",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::PromptLayer,
            "codex/prompts",
            "Codex prompt layer",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::TaskPolicy,
            "codex/tasks",
            "Codex task policy",
        ));
        registry.add_extension(LlmExtensionDescriptor::new(
            LlmExtensionKind::ToolPolicy,
            "codex/tools",
            "Codex tool dialect",
        ));
        registry.add_extension(tool_backend_extension(
            RESPONSES_WEB_SEARCH_BACKEND_ID,
            "Responses web_search",
        ));
        registry.add_profile(codex::profile());
    }
}

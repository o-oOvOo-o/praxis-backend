use crate::llm::internal_plugins::LlmPlugin;
use crate::llm::internal_plugins::LlmPluginDescriptor;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::tool_backend_extension;
use crate::llm::internal_plugins::tool_capability_extension;

const WEB_SEARCH_TOOL_ID: &str = "web_search";
const PRAXIS_WEB_SEARCH_BACKEND_ID: &str = "web_search/praxis";

pub(crate) struct WebSearchLlmPlugin;

impl LlmPlugin for WebSearchLlmPlugin {
    fn descriptor(&self) -> LlmPluginDescriptor {
        LlmPluginDescriptor {
            id: "web_search",
            label: "web_search",
        }
    }

    fn build(&self, registry: &mut LlmPluginRegistryBuilder) {
        registry.add_extension(tool_capability_extension(WEB_SEARCH_TOOL_ID, "web_search"));
        registry.add_extension(tool_backend_extension(
            PRAXIS_WEB_SEARCH_BACKEND_ID,
            "Praxis web_search",
        ));
    }
}

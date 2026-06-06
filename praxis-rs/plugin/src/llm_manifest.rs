use praxis_utils_absolute_path::AbsolutePathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginLlmManifest {
    pub profiles: Vec<PluginLlmProfile>,
    pub products: Vec<PluginLlmProduct>,
    pub tool_policies: Vec<PluginLlmToolPolicy>,
    pub model_catalogs: Vec<PluginLlmModelCatalog>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLlmProfile {
    pub id: String,
    pub provider: Option<String>,
    pub wire: Option<String>,
    pub behavior: Option<AbsolutePathBuf>,
    pub prompts: Vec<PluginLlmPromptSlot>,
    pub tasks: Option<AbsolutePathBuf>,
    pub tools: Option<AbsolutePathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLlmProduct {
    pub id: String,
    pub prompts: Vec<PluginLlmPromptSlot>,
    pub tasks: Option<AbsolutePathBuf>,
    pub tools: Option<AbsolutePathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLlmPromptSlot {
    pub slot: String,
    pub path: AbsolutePathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLlmToolPolicy {
    pub id: String,
    pub path: AbsolutePathBuf,
    pub applies_to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLlmModelCatalog {
    pub id: String,
    pub label: Option<String>,
    pub provider: Option<String>,
    pub wire: Option<String>,
    pub models: Vec<PluginLlmModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLlmModel {
    pub slug: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub context_window: Option<i64>,
    pub default_reasoning_effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
}

//! Wire protocol identity and compatibility policy.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;

const CHAT_WIRE_API_REMOVED_ERROR: &str = "`wire_api = \"chat\"` is no longer supported.\nHow to fix: set `wire_api = \"responses\"` in your provider config.";

/// Wire protocol that a provider speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    /// The Responses API exposed by OpenAI at `/v1/responses`.
    #[default]
    Responses,
    /// Anthropic/Claude-style messages API.
    Claude,
    /// Generic OpenAI-compatible chat/completions-style API.
    #[serde(rename = "openai_compat", alias = "common")]
    OpenAiCompat,
}

impl fmt::Display for WireApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Responses => "responses",
            Self::Claude => "claude",
            Self::OpenAiCompat => "openai_compat",
        };
        f.write_str(value)
    }
}

impl<'de> Deserialize<'de> for WireApi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "responses" => Ok(Self::Responses),
            "claude" => Ok(Self::Claude),
            "openai_compat" | "common" => Ok(Self::OpenAiCompat),
            "chat" => Err(serde::de::Error::custom(CHAT_WIRE_API_REMOVED_ERROR)),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["responses", "claude", "openai_compat"],
            )),
        }
    }
}

/// Provider-specific compatibility shims inside a broader wire family.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderCompatInfo {
    pub supports_developer_role: Option<bool>,
    pub supports_reasoning_effort: Option<bool>,
    pub reasoning_effort_map: Option<ModelProviderReasoningEffortMap>,
    pub supports_parallel_tool_calls: Option<bool>,
    pub max_tokens_field: Option<ModelProviderMaxTokensField>,
    pub max_tokens: Option<i64>,
    pub requires_tool_result_name: Option<bool>,
    pub requires_assistant_after_tool_result: Option<bool>,
    pub thinking_format: Option<ModelProviderThinkingFormat>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderReasoningEffortMap {
    pub minimal: Option<String>,
    pub low: Option<String>,
    pub medium: Option<String>,
    pub high: Option<String>,
    pub xhigh: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderMaxTokensField {
    MaxCompletionTokens,
    MaxTokens,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderThinkingFormat {
    Openai,
    Openrouter,
    Deepseek,
    Kimi,
    Gemini,
    Zai,
    Qwen,
    QwenChatTemplate,
    #[serde(alias = "llama_cpp_chat_template")]
    ChatTemplateKwargs,
}

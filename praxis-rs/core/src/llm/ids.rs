use praxis_protocol::protocol::Product;

use crate::model_provider_info::WireApi;

pub(crate) const OPENAI_RESPONSES_PROFILE_ID: &str = "openai/responses";
pub(crate) const OPENAI_RESPONSES_BASE_PROFILE_ID: &str = "openai/responses/base";
pub(crate) const LEGACY_CODEX_RESPONSES_PROFILE_ID: &str = "codex/responses";
pub(crate) const LEGACY_CODEX_RESPONSES_BASE_PROFILE_ID: &str = "codex/responses/base";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WireId {
    Responses,
    ClaudeMessages,
    OpenAiCompat,
}

impl WireId {
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ClaudeMessages => "claude_messages",
            Self::OpenAiCompat => "openai_compat",
        }
    }
}

impl From<WireApi> for WireId {
    fn from(api: WireApi) -> Self {
        match api {
            WireApi::Responses => Self::Responses,
            WireApi::Claude => Self::ClaudeMessages,
            WireApi::OpenAiCompat => Self::OpenAiCompat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BehaviorProfileId {
    OpenAiResponses,
    Common,
    DeepSeek,
    Gemini,
    Glm,
    Qwen,
    Claude,
    OpenRouter,
}

impl BehaviorProfileId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiResponses => OPENAI_RESPONSES_PROFILE_ID,
            Self::Common => "common",
            Self::DeepSeek => "deepseek",
            Self::Gemini => "gemini",
            Self::Glm => "glm",
            Self::Qwen => "qwen",
            Self::Claude => "claude",
            Self::OpenRouter => "openrouter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ProductProfileId(String);

impl ProductProfileId {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[cfg(test)]
    pub(crate) fn cunning3d() -> Self {
        Self::new(Product::CUNNING3D)
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn from_product(product: Product) -> Option<Self> {
        match product.as_str() {
            Product::CHATGPT | Product::ATLAS => None,
            product_id => Some(Self::new(product_id)),
        }
    }

    pub(crate) fn policy_reader_behavior_id(&self) -> BehaviorProfileId {
        BehaviorProfileId::Common
    }
}

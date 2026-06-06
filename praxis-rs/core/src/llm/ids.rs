use praxis_protocol::protocol::Product;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WireId {
    Responses,
    ClaudeMessages,
    OpenAiCompat,
}

impl WireId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ClaudeMessages => "claude_messages",
            Self::OpenAiCompat => "openai_compat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BehaviorProfileId {
    CodexResponses,
    Common,
    DeepSeek,
    Glm,
    Qwen,
    Claude,
    OpenRouter,
}

impl BehaviorProfileId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CodexResponses => "codex/responses",
            Self::Common => "common",
            Self::DeepSeek => "deepseek",
            Self::Glm => "glm",
            Self::Qwen => "qwen",
            Self::Claude => "claude",
            Self::OpenRouter => "openrouter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProductProfileId {
    Praxis,
    Cunning3d,
}

impl ProductProfileId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Praxis => "praxis",
            Self::Cunning3d => "cunning3d",
        }
    }

    pub(crate) fn from_product(product: Product) -> Option<Self> {
        match product {
            Product::Praxis => Some(Self::Praxis),
            Product::Cunning3d => Some(Self::Cunning3d),
            Product::Chatgpt | Product::Atlas => None,
        }
    }
}

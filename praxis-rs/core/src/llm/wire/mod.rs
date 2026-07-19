pub(crate) mod claude_messages;
mod compat;
pub(crate) mod openai_compat;
pub(crate) mod shared;

pub use compat::ModelProviderCompatInfo;
pub use compat::ModelProviderMaxTokensField;
pub use compat::ModelProviderReasoningEffortMap;
pub use compat::ModelProviderThinkingFormat;
pub use compat::WireApi;

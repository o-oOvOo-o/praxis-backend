use crate::api_bridge::CoreAuthProvider;
use crate::api_bridge::map_api_error;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::error::PraxisErr;
use crate::error::Result;
use crate::model_provider_info::ANTHROPIC_API_VERSION;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::ModelProviderMaxTokensField;
use crate::model_provider_info::ModelProviderReasoningEffortMap;
use crate::model_provider_info::ModelProviderThinkingFormat;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::HeaderMap;
use http::HeaderValue;
use http::header::AUTHORIZATION;
use http::header::CONTENT_TYPE;
use praxis_api::AuthProvider as ApiAuthProvider;
use praxis_api::Provider;
use praxis_api::TransportError;
use praxis_api::error::ApiError;
use praxis_login::default_client::build_direct_reqwest_client;
use praxis_login::default_client::build_reqwest_client;
use praxis_login::default_client::try_build_direct_reqwest_client_without_redirects;
use praxis_login::default_client::try_build_reqwest_client_without_redirects;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::LocalShellAction;
use praxis_protocol::models::ReasoningItemContent;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::protocol::TokenUsage;
use praxis_tools::JsonSchema;
use praxis_tools::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

const DEFAULT_CLAUDE_MAX_TOKENS: i64 = 16_384;
const DEFAULT_COMMON_MAX_TOKENS: i64 = 4096;
const CLAUDE_REASONING_BLOCK_PREFIX: &str = "praxis-anthropic-thinking-v1:";

pub(crate) fn is_claude_reasoning_content(content: &str) -> bool {
    content.starts_with(CLAUDE_REASONING_BLOCK_PREFIX)
}
const CLAUDE_TOOL_NAME_MAX_BYTES: usize = 64;
const COMMON_TOOL_RESULT_BRIDGE_MESSAGE: &str = "I have processed the tool results.";
const COMMON_POST_FINISH_GRACE_MS: u64 = 1_500;
const COMMON_DEEPSEEK_MESSAGE_IDLE_GRACE_MS: u64 = 15_000;
const COMMON_THINK_OPEN_TAG: &str = "<think>";
const COMMON_THINK_CLOSE_TAG: &str = "</think>";
const COMMON_THINK_TAG_TAIL_BYTES: usize = COMMON_THINK_CLOSE_TAG.len() - 1;
const COMMON_THINK_PRELUDE_BUFFER_BYTES: usize = 128;

mod common_request;
mod message_conversion;
mod parsing;
mod streaming;
mod transport;

use common_request::*;
use message_conversion::*;
use parsing::*;
use streaming::*;
use transport::ParsedProviderResponse;
pub(crate) use transport::stream_claude_unary;
pub(crate) use transport::stream_common_unary;

#[cfg(test)]
mod tests;

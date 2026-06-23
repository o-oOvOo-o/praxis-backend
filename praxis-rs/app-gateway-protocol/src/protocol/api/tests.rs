use super::*;
use praxis_protocol::items::AgentMessageContent;
use praxis_protocol::items::AgentMessageItem;
use praxis_protocol::items::ReasoningItem;
use praxis_protocol::items::TurnItem;
use praxis_protocol::items::UserMessageItem;
use praxis_protocol::items::WebSearchItem;
use praxis_protocol::models::WebSearchAction as CoreWebSearchAction;
use praxis_protocol::protocol::NetworkAccess as CoreNetworkAccess;
use praxis_protocol::protocol::ReadOnlyAccess as CoreReadOnlyAccess;
use praxis_protocol::user_input::UserInput as CoreUserInput;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::PathBuf;

fn absolute_path_string(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    if cfg!(windows) {
        format!(r"C:\{}", trimmed.replace('/', "\\"))
    } else {
        format!("/{trimmed}")
    }
}

fn absolute_path(path: &str) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(absolute_path_string(path)).expect("path must be absolute")
}

fn test_absolute_path() -> AbsolutePathBuf {
    absolute_path("readable")
}

mod guardian_requirements_items;
mod mcp_elicitation;
mod permissions_fs_command;
mod policy_experimental;
mod service_tier;
mod skills_plugins_errors;

use serde::Deserialize;
use serde::Serialize;

use crate::tool::ToolCall;
use crate::tool::ToolResult;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnItem {
    AssistantText {
        item_id: Option<String>,
        text: String,
    },
    Reasoning {
        item_id: Option<String>,
        text: String,
    },
    ToolCall(ToolCall),
    ToolStarted {
        call_id: String,
        name: String,
    },
    ToolProgress {
        call_id: String,
        content: String,
    },
    ToolResult(ToolResult),
    SystemText(String),
    UserText(String),
}

use serde::Deserialize;
use serde::Serialize;

use crate::tool::ToolResultStatus;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PromptItem {
    SystemText(String),
    UserText(String),
    AssistantText(String),
    ToolCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    ImageUrl(String),
    LocalImagePath(String),
    Skill {
        name: String,
        path: String,
    },
    Mention {
        name: String,
        path: String,
    },
    ToolResult {
        call_id: String,
        content: String,
        #[serde(rename = "is_error")]
        status: ToolResultStatus,
    },
    Opaque {
        format: String,
        data: String,
    },
}

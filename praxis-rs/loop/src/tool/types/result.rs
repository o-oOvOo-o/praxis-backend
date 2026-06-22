use serde::Deserialize;
use serde::Serialize;

use super::status::ToolResultStatus;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub content: String,
    #[serde(rename = "is_error")]
    pub status: ToolResultStatus,
}

impl ToolResult {
    pub fn success(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::with_status(call_id, content, ToolResultStatus::success())
    }

    pub fn error(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::with_status(call_id, content, ToolResultStatus::error())
    }

    pub fn with_status(
        call_id: impl Into<String>,
        content: impl Into<String>,
        status: ToolResultStatus,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            status,
        }
    }

    pub fn is_error(&self) -> bool {
        self.status.is_error()
    }

    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }
}

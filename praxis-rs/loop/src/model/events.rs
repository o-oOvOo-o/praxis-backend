use serde::Deserialize;
use serde::Serialize;

use crate::tool::ToolCall;
use crate::tool::ToolResult;

use super::usage::TokenUsage;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ModelEvent {
    TextDelta {
        item_id: Option<String>,
        text: String,
    },
    ReasoningDelta {
        item_id: Option<String>,
        summary_index: Option<i64>,
        content_index: Option<i64>,
        text: String,
    },
    ToolCall(ToolCall),
    FinalText {
        item_id: Option<String>,
        text: String,
    },
    RecordedFinalText {
        item_id: Option<String>,
        text: String,
    },
    FollowupRequired,
    Completed(TokenUsage),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnEvent {
    TextDelta {
        item_id: Option<String>,
        text: String,
    },
    ReasoningDelta {
        item_id: Option<String>,
        summary_index: Option<i64>,
        content_index: Option<i64>,
        text: String,
    },
    ToolStarted {
        call_id: String,
        name: String,
    },
    ToolFinished(ToolResult),
    ToolProgress {
        call_id: String,
        content: String,
    },
    TurnAborted(String),
    TurnCompleted,
}

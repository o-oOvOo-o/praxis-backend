use serde::Deserialize;
use serde::Serialize;

use crate::state::TurnState;
use crate::tool::ToolCall;

pub type LoopResult<T> = std::result::Result<T, TurnError>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnResult {
    Complete { state: TurnState },
    WantsFollowup { state: TurnState },
    Aborted { state: TurnState, reason: TurnError },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnError {
    pub kind: TurnErrorKind,
    pub message: String,
}

impl TurnError {
    pub fn new(kind: TurnErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self::new(TurnErrorKind::Cancelled, "turn was cancelled")
    }
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for TurnError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnErrorKind {
    Cancelled,
    Guard,
    Hook,
    Internal,
    Model,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RoundOutcome {
    Empty,
    FinalAnswer { message: TurnCompletionMessage },
    FollowupRequired,
    ToolCalls { calls: Vec<ToolCall> },
    TerminatedByTool { message: TurnCompletionMessage },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnCompletionMessage {
    #[default]
    NoMessage,
    Text(String),
}

impl TurnCompletionMessage {
    pub fn from_option(message: Option<String>) -> Self {
        match message {
            Some(message) => Self::Text(message),
            None => Self::NoMessage,
        }
    }

    pub fn text(message: impl Into<String>) -> Self {
        Self::Text(message.into())
    }

    pub fn into_option(self) -> Option<String> {
        match self {
            Self::NoMessage => None,
            Self::Text(message) => Some(message),
        }
    }
}

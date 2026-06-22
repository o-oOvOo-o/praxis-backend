use serde::Deserialize;
use serde::Serialize;

use crate::context::TurnContext;
use crate::context::TurnInput;
use crate::model::PromptItem;
use crate::model::TurnItem;
use crate::outcome::TurnCompletionMessage;
use crate::outcome::TurnError;
use crate::state::TokenLedger;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContextPressureDecision {
    Proceed,
    Compacted {
        prompt_items: Vec<PromptItem>,
        transcript_items: Vec<TurnItem>,
    },
    Abort(TurnError),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PrepareContextDecision {
    Prepared(Vec<PromptItem>),
    Stop(TurnCompletionMessage),
    Abort(TurnError),
}

#[derive(Clone, Copy, Debug)]
pub struct ContextPressureView<'a> {
    pub usage: &'a TokenLedger,
    pub context_window: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct PrepareContextView<'a> {
    pub ctx: &'a TurnContext,
    pub transcript_delta: &'a [TurnItem],
    pub input: &'a TurnInput,
}

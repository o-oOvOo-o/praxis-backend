use serde::Deserialize;
use serde::Serialize;

use crate::model::ModelSpec;
use crate::model::PromptItem;
use crate::outcome::RoundOutcome;
use crate::outcome::TurnCompletionMessage;
use crate::outcome::TurnError;
use crate::state::TokenLedger;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RoundDecision {
    Continue { prompt_update: RoundPromptUpdate },
    Stop(TurnCompletionMessage),
    Abort(TurnError),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RoundPromptUpdate {
    Reuse,
    Replace(Vec<PromptItem>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoundAdjustment {
    pub model: Option<ModelSpec>,
    pub reasoning: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PrepareNextRoundDecision {
    Reuse,
    Adjust(RoundAdjustment),
}

#[derive(Clone, Copy, Debug)]
pub struct RoundOutcomeView<'a> {
    pub outcome: &'a RoundOutcome,
    pub usage: &'a TokenLedger,
}

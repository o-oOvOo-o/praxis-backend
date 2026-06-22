use serde::Deserialize;
use serde::Serialize;

use crate::model::PromptItem;
use crate::outcome::TurnError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnStartDecision {
    Proceed,
    ReplaceInitialPrompt(Vec<PromptItem>),
    Abort(TurnError),
}

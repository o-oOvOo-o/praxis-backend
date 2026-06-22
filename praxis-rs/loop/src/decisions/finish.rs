use serde::Deserialize;
use serde::Serialize;

use crate::context::TurnContext;
use crate::outcome::TurnError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnStopDecision {
    Complete,
    ContinueTurn,
    Abort(TurnError),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnCompletionDecision {
    Complete,
    WantsFollowup,
}

#[derive(Clone, Copy, Debug)]
pub struct TurnStopView<'a> {
    pub ctx: &'a TurnContext,
    pub last_agent_message: Option<&'a str>,
}

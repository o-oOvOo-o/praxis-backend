use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;

use crate::model::SteeringMessage;
use crate::outcome::LoopResult;
use crate::outcome::TurnCompletionMessage;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SteeringDrain {
    pub messages: Vec<SteeringMessage>,
    pub control: SteeringControl,
}

impl SteeringDrain {
    pub fn empty() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum SteeringControl {
    #[default]
    Continue,
    RetryWithoutModelRequest,
    StopWithoutModelRequest(TurnCompletionMessage),
}

#[async_trait]
pub trait SteeringInbox: Send + Sync {
    async fn drain_steering(&self) -> LoopResult<SteeringDrain>;
}

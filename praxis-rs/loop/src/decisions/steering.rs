use serde::Deserialize;
use serde::Serialize;

use crate::model::SteeringMessage;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SteeringDecision {
    InjectAndContinue,
    DropAndContinue,
}

#[derive(Clone, Copy, Debug)]
pub struct SteeringInputView<'a> {
    pub messages: &'a [SteeringMessage],
}

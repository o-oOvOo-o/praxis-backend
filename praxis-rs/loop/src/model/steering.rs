use serde::Deserialize;
use serde::Serialize;

use super::prompt_item::PromptItem;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SteeringMessage {
    pub prompt_items: Vec<PromptItem>,
}

impl SteeringMessage {
    pub fn new(prompt_items: Vec<PromptItem>) -> Self {
        Self { prompt_items }
    }
}

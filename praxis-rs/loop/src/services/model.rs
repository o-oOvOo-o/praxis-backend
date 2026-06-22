use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::Deserialize;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::ids::TurnId;
use crate::model::ModelEvent;
use crate::model::ModelSpec;
use crate::model::PromptItem;
use crate::outcome::LoopResult;

pub type ModelEventStream = Pin<Box<dyn Stream<Item = LoopResult<ModelEvent>> + Send>>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoundSettings {
    pub model: ModelSpec,
    pub reasoning: Option<String>,
    pub service_tier: Option<String>,
}

impl RoundSettings {
    pub fn new(model: ModelSpec) -> Self {
        Self {
            model,
            reasoning: None,
            service_tier: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub turn_id: TurnId,
    pub round: u64,
    pub settings: RoundSettings,
    pub prompt: Vec<PromptItem>,
}

#[async_trait]
pub trait ModelService: Send + Sync {
    async fn stream_model(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
    ) -> LoopResult<ModelEventStream>;
}

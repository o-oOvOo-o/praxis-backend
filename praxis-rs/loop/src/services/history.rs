use async_trait::async_trait;

use crate::model::TurnItem;
use crate::outcome::LoopResult;

#[async_trait]
pub trait HistorySink: Send + Sync {
    async fn persist_items(&self, items: &[TurnItem]) -> LoopResult<()>;
}

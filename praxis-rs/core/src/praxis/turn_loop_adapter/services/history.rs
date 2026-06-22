use async_trait::async_trait;
use praxis_loop::model::TurnItem as LoopTurnItem;
use praxis_loop::outcome::LoopResult;
use praxis_loop::services::HistorySink;

use super::super::history_bridge;
use super::PraxisTurnServices;

#[async_trait]
impl HistorySink for PraxisTurnServices {
    async fn persist_items(&self, items: &[LoopTurnItem]) -> LoopResult<()> {
        let response_items = history_bridge::loop_turn_items_to_response_items(items);
        for response_item in response_items {
            self.session
                .record_response_item_and_emit_turn_item(&self.turn_context, response_item)
                .await;
        }
        Ok(())
    }
}

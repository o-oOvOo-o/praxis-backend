use async_trait::async_trait;
use praxis_loop::model::TurnEvent;
use praxis_loop::outcome::LoopResult;
use praxis_loop::services::EventSink;

use super::PraxisTurnServices;
use super::loop_event_sink_projection;

#[async_trait]
impl EventSink for PraxisTurnServices {
    async fn emit_event(&self, event: TurnEvent) -> LoopResult<()> {
        loop_event_sink_projection::emit_loop_event(
            self.session(),
            self.turn_context(),
            self.turn_diff_tracker().await,
            event,
        )
        .await;
        Ok(())
    }
}

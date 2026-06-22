use async_trait::async_trait;
use praxis_loop::outcome::LoopResult;
use praxis_loop::services::SteeringDrain;
use praxis_loop::services::SteeringInbox;

use super::PraxisTurnServices;

#[async_trait]
impl SteeringInbox for PraxisTurnServices {
    async fn drain_steering(&self) -> LoopResult<SteeringDrain> {
        Ok(SteeringDrain {
            messages: Vec::new(),
            control: self
                .process_pending_input_for_round()
                .await
                .into_loop_control(),
        })
    }
}

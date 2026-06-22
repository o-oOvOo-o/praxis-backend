use std::sync::Arc;

use async_trait::async_trait;
use praxis_loop::outcome::LoopResult;
use praxis_loop::services::ModelEventStream;
use praxis_loop::services::ModelRequest;
use praxis_loop::services::ModelService;
use tokio_util::sync::CancellationToken;

use super::super::model_stream;
use super::super::model_stream::PraxisModelStreamInput;
use super::PraxisTurnServices;

#[async_trait]
impl ModelService for PraxisTurnServices {
    async fn stream_model(
        &self,
        request: ModelRequest,
        cancellation_token: CancellationToken,
    ) -> LoopResult<ModelEventStream> {
        model_stream::stream_model(
            PraxisModelStreamInput {
                session: Arc::clone(&self.session),
                turn_context: Arc::clone(&self.turn_context),
                bridge_state: Arc::clone(&self.bridge_state),
                runtime_state: Arc::clone(&self.runtime_state),
                tool_runtime_slot: self.tool_runtime_slot.clone(),
            },
            request,
            cancellation_token,
        )
        .await
    }
}

use std::sync::Arc;

use praxis_loop::outcome::LoopResult;
use praxis_loop::services::ModelEventStream;
use praxis_loop::services::ModelRequest;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::super::Session;
use super::super::TurnContext;
use super::model_round_state::PraxisModelRoundState;
use super::state::PraxisTurnBridgeState;
use super::tool_runtime_slot::ModelRoundToolsSlot;

mod assistant_text_stream;
mod code_mode_worker;
mod completed_tool_call;
mod completed_tool_call_conversion;
mod error_bridge;
mod function_call_error_projection;
mod item_completion;
mod model_round;
mod non_tool_item;
mod plan_mode_stream;
mod provider_projection;
mod provider_stream;
mod reasoning_delta_stream;
mod request_context;
mod request_context_update;
mod request_settings;
mod request_telemetry;
mod response_item_identity;
mod stream_item_completion;
mod stream_item_delta;
mod stream_item_start;
mod stream_item_state;
mod stream_run_state;
mod token_usage_bridge;
mod tool_error_response;

pub(super) struct PraxisModelStreamInput {
    pub(super) session: Arc<Session>,
    pub(super) turn_context: Arc<TurnContext>,
    pub(super) bridge_state: Arc<PraxisTurnBridgeState>,
    pub(super) runtime_state: Arc<Mutex<PraxisModelRoundState>>,
    pub(super) tool_runtime_slot: ModelRoundToolsSlot,
}

pub(super) async fn stream_model(
    input: PraxisModelStreamInput,
    request: ModelRequest,
    cancellation_token: CancellationToken,
) -> LoopResult<ModelEventStream> {
    let round = model_round::prepare_model_round(input, request, &cancellation_token).await?;

    Ok(provider_stream::open_event_stream(
        round.input,
        round.prompt,
        round.turn_metadata_header,
        cancellation_token,
        round.code_mode_worker,
    ))
}

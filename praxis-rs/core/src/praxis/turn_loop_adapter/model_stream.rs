#![allow(unused_imports)]

use std::sync::Arc;

use praxis_loop::outcome::LoopResult;
use praxis_loop::services::ModelEventStream;
use praxis_loop::services::ModelRequest;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::super::Session;
use super::super::TurnContext;
use super::model_round_state::PraxisModelRoundState;
use super::round_input;
use super::state::PraxisTurnBridgeState;
use super::tool_call_bridge;
use super::tool_runtime_slot::ModelRoundToolsSlot;

mod assistant_stream;
mod provider_events;
mod provider_transport;
mod request_runtime;
mod round_runtime;
mod stream_items;

use assistant_stream::assistant_text_stream;
use assistant_stream::plan_mode_stream;
use assistant_stream::reasoning_delta_stream;
use provider_events::completed_tool_call;
use provider_events::completed_tool_call_conversion;
use provider_events::function_call_error_projection;
use provider_events::item_completion;
use provider_events::provider_projection;
use provider_events::token_usage_bridge;
use provider_transport::error_bridge;
use provider_transport::provider_stream;
use provider_transport::request_telemetry;
use provider_transport::stream_run_state;
use request_runtime::request_context;
use request_runtime::request_context_update;
use request_runtime::request_settings;
use round_runtime::code_mode_worker;
use round_runtime::model_round;
use round_runtime::tool_error_response;
use stream_items::non_tool_item;
use stream_items::response_item_identity;
use stream_items::stream_item_completion;
use stream_items::stream_item_delta;
use stream_items::stream_item_start;
use stream_items::stream_item_state;

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

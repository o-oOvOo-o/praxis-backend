use praxis_loop::outcome::LoopResult;
use praxis_loop::services::ModelRequest;
use tokio_util::sync::CancellationToken;

use crate::client_common::Prompt;

use super::PraxisModelStreamInput;
use super::request_context;

mod input_projection;
mod prompt;
mod tool_runtime;
mod tooling;
mod tools;

pub(super) struct PreparedModelRound {
    pub(super) input: PraxisModelStreamInput,
    pub(super) prompt: Prompt,
    pub(super) turn_metadata_header: Option<String>,
    pub(super) code_mode_worker: Option<praxis_code_mode::CodeModeTurnWorker>,
}

pub(super) async fn prepare_model_round(
    mut input: PraxisModelStreamInput,
    request: ModelRequest,
    cancellation_token: &CancellationToken,
) -> LoopResult<PreparedModelRound> {
    input.turn_context = request_context::resolve_request_turn_context(
        &input.session,
        &input.turn_context,
        &request,
    )
    .await?;
    tracing::trace!(
        round = request.round,
        model = input.turn_context.model_info.slug.as_str(),
        reasoning = ?input.turn_context.reasoning_effort,
        service_tier = ?input.turn_context.config.service_tier,
        loop_prompt_items = request.prompt.len(),
        "building Praxis provider prompt from loop request"
    );
    let round_input = input_projection::project_round_input(&input, &request).await;

    let tooling = tooling::prepare_tooling(&input, round_input.items, cancellation_token).await?;

    Ok(PreparedModelRound {
        input,
        prompt: tooling.prompt,
        turn_metadata_header: round_input.turn_metadata_header,
        code_mode_worker: tooling.code_mode_worker,
    })
}

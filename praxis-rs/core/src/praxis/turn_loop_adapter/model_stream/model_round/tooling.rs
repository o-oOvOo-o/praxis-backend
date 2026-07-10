use std::sync::Arc;

use praxis_loop::outcome::LoopResult;
use praxis_protocol::models::ResponseItem;
use tokio_util::sync::CancellationToken;

use crate::client_common::Prompt;
use crate::tools::code_mode::CodeModeTurnWorker;

use super::super::PraxisModelStreamInput;
use super::super::code_mode_worker;
use super::prompt;
use super::tools;

pub(super) struct PreparedTooling {
    pub(super) prompt: Prompt,
    pub(super) code_mode_worker: Option<CodeModeTurnWorker>,
}

pub(super) async fn prepare_tooling(
    input: &PraxisModelStreamInput,
    items: Vec<ResponseItem>,
    cancellation_token: &CancellationToken,
) -> LoopResult<PreparedTooling> {
    let explicitly_enabled_connectors = input.bridge_state.explicitly_enabled_connectors().await;
    let turn_diff_tracker = input.runtime_state.lock().await.turn_diff_tracker();
    let tools = tools::build_tools(
        &input.session,
        &input.turn_context,
        Arc::clone(&turn_diff_tracker),
        &items,
        &explicitly_enabled_connectors,
        cancellation_token,
    )
    .await?;

    let prompt = prompt::build_provider_prompt(
        input.session.as_ref(),
        input.turn_context.as_ref(),
        items,
        tools.router(),
    )
    .await;

    input.tool_runtime_slot.store(tools.runtime())?;

    let code_mode_worker = code_mode_worker::start_turn_worker(
        &input.session,
        &input.turn_context,
        tools.router_arc(),
        Arc::clone(&turn_diff_tracker),
    )
    .await;

    Ok(PreparedTooling {
        prompt,
        code_mode_worker,
    })
}

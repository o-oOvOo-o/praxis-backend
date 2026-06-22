use praxis_loop::outcome::LoopResult;
use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;
use praxis_protocol::models::ResponseItem;

use crate::function_tool::FunctionCallError;

use super::PraxisModelStreamInput;
use super::tool_error_response::record_tool_error_response;

pub(super) async fn project_function_call_error(
    input: &PraxisModelStreamInput,
    source_item: &ResponseItem,
    err: FunctionCallError,
) -> LoopResult<()> {
    match err {
        FunctionCallError::MissingLocalShellCallId => {
            record_missing_local_shell_call_id(input, source_item).await;
            Ok(())
        }
        FunctionCallError::RespondToModel(message) => {
            record_tool_error_response(input, source_item, message).await;
            Ok(())
        }
        FunctionCallError::Fatal(message) => Err(TurnError::new(TurnErrorKind::Tool, message)),
    }
}

async fn record_missing_local_shell_call_id(
    input: &PraxisModelStreamInput,
    source_item: &ResponseItem,
) {
    const MESSAGE: &str = "LocalShellCall without call_id or id";
    input
        .turn_context
        .session_telemetry
        .log_tool_failed("local_shell", MESSAGE);
    tracing::error!("{MESSAGE}");
    record_tool_error_response(input, source_item, MESSAGE).await;
}

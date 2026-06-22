use crate::error::PraxisErr;

use super::super::PraxisModelStreamInput;

pub(super) async fn maybe_notify_retry(
    input: &PraxisModelStreamInput,
    retries: u64,
    max_retries: u64,
    err: PraxisErr,
) {
    if !should_report_retry(input, retries) {
        return;
    }

    input
        .session
        .notify_stream_error(
            &input.turn_context,
            format!("Reconnecting... {retries}/{max_retries}"),
            err,
        )
        .await;
}

fn should_report_retry(input: &PraxisModelStreamInput, retries: u64) -> bool {
    retries > 1
        || cfg!(debug_assertions)
        || !input
            .session
            .services
            .model_runtime
            .responses_websocket_enabled_for(
                &input.turn_context.config.model_provider_id,
                &input.turn_context.provider,
            )
}

use tokio_util::sync::CancellationToken;

use crate::client_common::Prompt;
use crate::error::PraxisErr;

use super::super::PraxisModelStreamInput;
use super::super::request_telemetry::record_model_request_start;
use super::driver::ProviderStreamDriver;

pub(super) async fn open_driver(
    input: &PraxisModelStreamInput,
    prompt: &Prompt,
    turn_metadata_header: Option<&str>,
    cancellation_token: &CancellationToken,
) -> Result<ProviderStreamDriver, PraxisErr> {
    record_model_request_start(input.session.as_ref(), input.turn_context.as_ref());
    let mut runtime_state = input.runtime_state.lock().await;
    ProviderStreamDriver::open(
        runtime_state.client_session_mut(),
        &input.turn_context,
        prompt,
        turn_metadata_header,
        cancellation_token,
    )
    .await
}

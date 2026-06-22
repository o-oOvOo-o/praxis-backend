use praxis_loop::outcome::LoopResult;
use tokio_util::sync::CancellationToken;

use super::driver::ProviderStreamDriver;
use super::driver::ReceivedResponseEvent;
use crate::client_common::Prompt;
use crate::error::PraxisErr;

use super::super::PraxisModelStreamInput;
use super::super::stream_run_state::ProviderStreamRunState;
use super::opening;
use super::retry;

pub(super) enum DriverOpenStep {
    Opened(ProviderStreamDriver),
    RetryAfterWait,
}

pub(super) enum EventReadStep {
    Received(ReceivedResponseEvent),
    RetryAfterWait,
}

pub(super) async fn open_or_wait_for_retry(
    input: &PraxisModelStreamInput,
    prompt: &Prompt,
    turn_metadata_header: Option<&str>,
    cancellation_token: &CancellationToken,
    run_state: &mut ProviderStreamRunState,
) -> LoopResult<DriverOpenStep> {
    match opening::open_driver(input, prompt, turn_metadata_header, cancellation_token).await {
        Ok(driver) => Ok(DriverOpenStep::Opened(driver)),
        Err(err) => {
            wait_before_retry(input, err, run_state).await?;
            Ok(DriverOpenStep::RetryAfterWait)
        }
    }
}

pub(super) async fn next_event_or_wait_for_retry(
    input: &PraxisModelStreamInput,
    driver: &mut ProviderStreamDriver,
    cancellation_token: &CancellationToken,
    run_state: &mut ProviderStreamRunState,
) -> LoopResult<EventReadStep> {
    match driver
        .next_event(&input.session, &input.turn_context, cancellation_token)
        .await
    {
        Ok(received) => Ok(EventReadStep::Received(received)),
        Err(err) => {
            wait_before_retry(input, err, run_state).await?;
            Ok(EventReadStep::RetryAfterWait)
        }
    }
}

async fn wait_before_retry(
    input: &PraxisModelStreamInput,
    err: PraxisErr,
    run_state: &mut ProviderStreamRunState,
) -> LoopResult<()> {
    let progress = run_state.model_stream_progress();
    retry::wait_before_retry_or_error(input, err, run_state.retry_count_mut(), progress).await
}

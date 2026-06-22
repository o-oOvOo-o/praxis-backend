use praxis_loop::outcome::LoopResult;
use tracing::warn;

use crate::error::PraxisErr;
use crate::util::backoff;

use super::super::PraxisModelStreamInput;
use super::super::error_bridge::finish_model_error;
use super::super::stream_run_state::ModelStreamProgress;
use super::retry_notice;
use super::retry_transport;

pub(super) async fn wait_before_retry_or_error(
    input: &PraxisModelStreamInput,
    err: PraxisErr,
    retries: &mut u64,
    progress: ModelStreamProgress,
) -> LoopResult<()> {
    if progress.has_model_output() || !err.is_retryable() {
        return Err(finish_model_error(input, err).await);
    }

    let max_retries = input.turn_context.provider.stream_max_retries();
    if *retries >= max_retries && retry_transport::switch_to_fallback_transport(input).await {
        retry_transport::warn_fallback_transport(input, &err).await;
        *retries = 0;
        return Ok(());
    }

    if *retries < max_retries {
        *retries += 1;
        let delay = retry_delay(&err, *retries);
        warn!(
            "stream disconnected - retrying model request ({retries}/{max_retries} in {delay:?})...",
        );

        retry_notice::maybe_notify_retry(input, *retries, max_retries, err).await;
        tokio::time::sleep(delay).await;
        return Ok(());
    }

    Err(finish_model_error(input, err).await)
}

fn retry_delay(err: &PraxisErr, retries: u64) -> std::time::Duration {
    match err {
        PraxisErr::Stream(_, requested_delay) => {
            requested_delay.unwrap_or_else(|| backoff(retries))
        }
        _ => backoff(retries),
    }
}

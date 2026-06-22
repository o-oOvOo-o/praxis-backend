use async_stream::try_stream;
use praxis_loop::services::ModelEventStream;
use tokio_util::sync::CancellationToken;

use super::stream_item_state::StreamItemState;
use crate::client_common::Prompt;

use self::retrying::DriverOpenStep;
use self::retrying::EventReadStep;
use super::PraxisModelStreamInput;
use super::provider_projection;
use super::provider_projection::ProviderStreamStep;
use super::stream_run_state::ProviderStreamRunState;

mod driver;
mod opening;
mod retry;
mod retry_notice;
mod retry_transport;
mod retrying;

pub(super) fn open_event_stream(
    input: PraxisModelStreamInput,
    prompt: Prompt,
    turn_metadata_header: Option<String>,
    cancellation_token: CancellationToken,
    code_mode_worker: Option<praxis_code_mode::CodeModeTurnWorker>,
) -> ModelEventStream {
    let stream = try_stream! {
        let input = input;
        let prompt = prompt;
        let turn_metadata_header = turn_metadata_header;
        let cancellation_token = cancellation_token;
        let _code_mode_worker = code_mode_worker;
        let mut run_state = ProviderStreamRunState::default();

        loop {
            let mut driver = match retrying::open_or_wait_for_retry(
                &input,
                &prompt,
                turn_metadata_header.as_deref(),
                &cancellation_token,
                &mut run_state,
            )
            .await? {
                DriverOpenStep::Opened(driver) => driver,
                DriverOpenStep::RetryAfterWait => continue,
            };
            let mut stream_items = StreamItemState::new(&input.turn_context);

            loop {
                let received = match retrying::next_event_or_wait_for_retry(
                    &input,
                    &mut driver,
                    &cancellation_token,
                    &mut run_state,
                )
                .await? {
                    EventReadStep::Received(received) => received,
                    EventReadStep::RetryAfterWait => break,
                };

                let projected = provider_projection::project_response_event(
                    &input,
                    &mut stream_items,
                    received.event,
                )
                .await?;
                run_state.observe_model_output(projected.observed_model_output);

                match projected.step {
                    ProviderStreamStep::Yield(event) => {
                        yield event;
                    }
                    ProviderStreamStep::Finish(event) => {
                        yield event;
                        return;
                    }
                    ProviderStreamStep::Continue => {}
                }
            }
        }
    };

    Box::pin(stream)
}

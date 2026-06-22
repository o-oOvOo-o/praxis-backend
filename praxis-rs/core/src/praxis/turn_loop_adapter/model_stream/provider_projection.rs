use praxis_loop::outcome::LoopResult;

use super::stream_item_state::StreamItemState;
use crate::client_common::ResponseEvent;

pub(super) use self::event::ModelOutputObservation;
pub(super) use self::event::ProviderEventProjection;
pub(super) use self::stream_step::ProviderStreamStep;
use super::PraxisModelStreamInput;

mod completion;
mod effect;
mod effect_application;
mod event;
mod event_handler;
mod incremental;
mod response_event;
mod stream_step;
mod terminal;

pub(super) struct ProjectedProviderEvent {
    pub(super) step: ProviderStreamStep,
    pub(super) observed_model_output: ModelOutputObservation,
}

pub(super) async fn project_response_event(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    event: ResponseEvent,
) -> LoopResult<ProjectedProviderEvent> {
    let projection = event_handler::handle_provider_event(input, stream_items, event).await?;
    let observed_model_output = projection.observed_model_output();
    let step = stream_step::apply_provider_projection(input, stream_items, projection).await;

    Ok(ProjectedProviderEvent {
        step,
        observed_model_output,
    })
}

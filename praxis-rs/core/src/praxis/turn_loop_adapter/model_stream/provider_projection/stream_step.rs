use praxis_loop::model::ModelEvent;

use super::super::stream_item_state::StreamItemState;

use super::super::PraxisModelStreamInput;
use super::completion;
use super::effect_application;
use super::event::ProviderEventProjection;

pub(in super::super) enum ProviderStreamStep {
    Yield(ModelEvent),
    Finish(ModelEvent),
    Continue,
}

pub(super) async fn apply_provider_projection(
    input: &PraxisModelStreamInput,
    stream_items: &mut StreamItemState,
    projection: ProviderEventProjection,
) -> ProviderStreamStep {
    match projection {
        ProviderEventProjection::Loop(event) => ProviderStreamStep::Yield(event),
        ProviderEventProjection::Completed {
            protocol_usage,
            loop_usage,
        } => {
            let event = completion::finish_completed_stream(
                input,
                stream_items,
                protocol_usage,
                loop_usage,
            )
            .await;
            ProviderStreamStep::Finish(event)
        }
        ProviderEventProjection::CoreEffect { effect, .. } => {
            effect_application::apply_core_effect(input, effect).await;
            ProviderStreamStep::Continue
        }
        ProviderEventProjection::Ignore { .. } => ProviderStreamStep::Continue,
    }
}

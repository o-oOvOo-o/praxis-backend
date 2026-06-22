use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

use super::effect::ProviderEffect;
use super::event::ModelOutputObservation;
use super::event::ProviderEventProjection;

pub(super) fn completed(token_usage: Option<ProtocolTokenUsage>) -> ProviderEventProjection {
    ProviderEventProjection::completed(token_usage)
}

pub(super) fn effect(effect: ProviderEffect) -> ProviderEventProjection {
    ProviderEventProjection::core_effect(effect, ModelOutputObservation::NotObserved)
}

pub(super) fn ignore() -> ProviderEventProjection {
    ProviderEventProjection::ignore(ModelOutputObservation::NotObserved)
}

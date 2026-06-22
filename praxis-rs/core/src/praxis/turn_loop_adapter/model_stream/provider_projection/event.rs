use praxis_loop::model::ModelEvent;
use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

use super::effect::ProviderEffect;

use super::super::token_usage_bridge;

pub(in super::super) enum ProviderEventProjection {
    Loop(ModelEvent),
    Completed {
        protocol_usage: Option<ProtocolTokenUsage>,
        loop_usage: praxis_loop::model::TokenUsage,
    },
    CoreEffect {
        effect: ProviderEffect,
        observation: ModelOutputObservation,
    },
    Ignore {
        observation: ModelOutputObservation,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in super::super) enum ModelOutputObservation {
    Observed,
    NotObserved,
}

impl ModelOutputObservation {
    pub(in super::super) const fn as_bool(self) -> bool {
        matches!(self, Self::Observed)
    }
}

impl ProviderEventProjection {
    pub(in super::super) fn loop_event(event: ModelEvent) -> Self {
        Self::Loop(event)
    }

    pub(super) fn completed(protocol_usage: Option<ProtocolTokenUsage>) -> Self {
        let loop_usage = token_usage_bridge::protocol_to_loop(protocol_usage.as_ref());
        Self::Completed {
            protocol_usage,
            loop_usage,
        }
    }

    pub(super) fn core_effect(effect: ProviderEffect, observation: ModelOutputObservation) -> Self {
        Self::CoreEffect {
            effect,
            observation,
        }
    }

    pub(in super::super) fn ignore(observation: ModelOutputObservation) -> Self {
        Self::Ignore { observation }
    }

    pub(super) fn observed_model_output(&self) -> ModelOutputObservation {
        match self {
            ProviderEventProjection::Loop(_) | ProviderEventProjection::Completed { .. } => {
                ModelOutputObservation::Observed
            }
            ProviderEventProjection::CoreEffect { observation, .. }
            | ProviderEventProjection::Ignore { observation } => *observation,
        }
    }
}

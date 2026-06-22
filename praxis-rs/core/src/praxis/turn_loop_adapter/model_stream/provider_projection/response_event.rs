use super::effect::ProviderEffect;
use crate::client_common::ResponseEvent;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::TokenUsage;

pub(super) enum ModelResponseEvent {
    Ignore,
    ItemAdded(ResponseItem),
    ItemDone(ResponseItem),
    TextDelta(String),
    ReasoningSummaryDelta { delta: String, summary_index: i64 },
    ReasoningSummaryPartAdded { summary_index: i64 },
    ReasoningContentDelta { delta: String, content_index: i64 },
    Completed { token_usage: Option<TokenUsage> },
    Effect(ProviderEffect),
}

pub(super) fn classify_response_event(event: ResponseEvent) -> ModelResponseEvent {
    match event {
        ResponseEvent::Created => ModelResponseEvent::Ignore,
        ResponseEvent::OutputItemAdded(item) => ModelResponseEvent::ItemAdded(item),
        ResponseEvent::OutputItemDone(item) => ModelResponseEvent::ItemDone(item),
        ResponseEvent::OutputTextDelta(delta) => ModelResponseEvent::TextDelta(delta),
        ResponseEvent::ReasoningSummaryDelta {
            delta,
            summary_index,
        } => ModelResponseEvent::ReasoningSummaryDelta {
            delta,
            summary_index,
        },
        ResponseEvent::ReasoningSummaryPartAdded { summary_index } => {
            ModelResponseEvent::ReasoningSummaryPartAdded { summary_index }
        }
        ResponseEvent::ReasoningContentDelta {
            delta,
            content_index,
        } => ModelResponseEvent::ReasoningContentDelta {
            delta,
            content_index,
        },
        ResponseEvent::Completed { token_usage, .. } => {
            ModelResponseEvent::Completed { token_usage }
        }
        ResponseEvent::ServerModel(server_model) => {
            ModelResponseEvent::Effect(ProviderEffect::ServerModel(server_model))
        }
        ResponseEvent::ServerReasoningIncluded(included) => {
            ModelResponseEvent::Effect(ProviderEffect::ServerReasoningIncluded(included))
        }
        ResponseEvent::RateLimits(snapshot) => {
            ModelResponseEvent::Effect(ProviderEffect::RateLimits(snapshot))
        }
        ResponseEvent::ModelsEtag(etag) => {
            ModelResponseEvent::Effect(ProviderEffect::ModelsEtag(etag))
        }
    }
}

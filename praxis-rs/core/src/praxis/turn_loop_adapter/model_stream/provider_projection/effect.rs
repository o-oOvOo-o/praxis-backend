use std::sync::Arc;

use praxis_protocol::protocol::AgentReasoningSectionBreakEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RateLimitSnapshot;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::util::error_or_panic;

#[derive(Debug)]
pub(in super::super) enum ProviderEffect {
    ServerModel(String),
    ServerReasoningIncluded(bool),
    RateLimits(RateLimitSnapshot),
    ModelsEtag(String),
    ReasoningSummaryPartAdded {
        item_id: Option<String>,
        summary_index: i64,
    },
}

pub(super) async fn apply_provider_effect(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    effect: ProviderEffect,
    server_model_warning_emitted_for_turn: &mut bool,
) {
    match effect {
        ProviderEffect::ServerModel(server_model) => {
            if !*server_model_warning_emitted_for_turn
                && sess
                    .maybe_warn_on_server_model_mismatch(turn_context, server_model)
                    .await
            {
                *server_model_warning_emitted_for_turn = true;
            }
        }
        ProviderEffect::ServerReasoningIncluded(included) => {
            sess.set_server_reasoning_included(included).await;
        }
        ProviderEffect::RateLimits(snapshot) => {
            sess.update_rate_limits(turn_context, snapshot).await;
        }
        ProviderEffect::ModelsEtag(etag) => {
            sess.services.models_manager.refresh_if_new_etag(etag).await;
        }
        ProviderEffect::ReasoningSummaryPartAdded {
            item_id,
            summary_index,
        } => {
            let Some(item_id) = item_id else {
                error_or_panic("ReasoningSummaryPartAdded without active item".to_string());
                return;
            };
            sess.send_event(
                turn_context,
                EventMsg::AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent {
                    item_id,
                    summary_index,
                }),
            )
            .await;
        }
    }
}

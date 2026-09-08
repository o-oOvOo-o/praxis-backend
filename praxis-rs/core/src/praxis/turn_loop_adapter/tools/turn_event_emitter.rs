mod assistant_text {
    use std::sync::Arc;

    use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
    use praxis_protocol::protocol::EventMsg;

    use super::super::super::Session;
    use super::super::super::TurnContext;
    use super::super::event_scope::TurnEventScope;

    pub(in crate::praxis::turn_loop_adapter) async fn emit_text_delta(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        item_id: Option<String>,
        text: String,
    ) {
        let scope = TurnEventScope::new(session, turn_context);
        let Some(item_id) = scope.active_item_id(item_id, "TextDelta") else {
            return;
        };

        session
            .send_event(
                turn_context,
                EventMsg::AgentMessageContentDelta(AgentMessageContentDeltaEvent {
                    thread_id: scope.thread_id(),
                    turn_id: scope.turn_id(),
                    item_id,
                    delta: text,
                }),
            )
            .await;
    }
}
mod reasoning {
    use std::sync::Arc;

    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::ReasoningContentDeltaEvent;
    use praxis_protocol::protocol::ReasoningRawContentDeltaEvent;

    use super::super::super::Session;
    use super::super::super::TurnContext;
    use super::super::event_scope::TurnEventScope;

    pub(in crate::praxis::turn_loop_adapter) async fn emit_reasoning_delta(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        item_id: Option<String>,
        summary_index: Option<i64>,
        content_index: Option<i64>,
        text: String,
    ) {
        let scope = TurnEventScope::new(session, turn_context);
        let Some(item_id) = scope.active_item_id(item_id, "ReasoningDelta") else {
            return;
        };

        if let Some(content_index) = content_index {
            session
                .send_event(
                    turn_context,
                    EventMsg::ReasoningRawContentDelta(ReasoningRawContentDeltaEvent {
                        thread_id: scope.thread_id(),
                        turn_id: scope.turn_id(),
                        item_id,
                        delta: text,
                        content_index,
                    }),
                )
                .await;
            return;
        }

        session
            .send_event(
                turn_context,
                EventMsg::ReasoningContentDelta(ReasoningContentDeltaEvent {
                    thread_id: scope.thread_id(),
                    turn_id: scope.turn_id(),
                    item_id,
                    delta: text,
                    summary_index: summary_index.unwrap_or_default(),
                }),
            )
            .await;
    }
}
mod turn_diff {
    use std::sync::Arc;

    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::TurnDiffEvent;

    use crate::tools::context::SharedTurnDiffTracker;

    use super::super::super::Session;
    use super::super::super::TurnContext;

    pub(in crate::praxis::turn_loop_adapter) async fn emit_turn_diff_if_present(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        turn_diff_tracker: &SharedTurnDiffTracker,
    ) {
        let unified_diff = {
            let mut tracker = turn_diff_tracker.lock().await;
            tracker.get_unified_diff()
        };

        if let Ok(Some(unified_diff)) = unified_diff {
            session
                .send_event(
                    turn_context,
                    EventMsg::TurnDiff(TurnDiffEvent { unified_diff }),
                )
                .await;
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) use assistant_text::emit_text_delta;
pub(in crate::praxis::turn_loop_adapter) use reasoning::emit_reasoning_delta;
pub(in crate::praxis::turn_loop_adapter) use turn_diff::emit_turn_diff_if_present;

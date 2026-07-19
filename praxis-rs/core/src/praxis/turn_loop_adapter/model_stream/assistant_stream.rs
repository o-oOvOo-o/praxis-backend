//! Assistant text, reasoning, and plan-mode stream assembly.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter::model_stream) mod assistant_text_stream {
    mod emitter {
        use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
        use praxis_protocol::protocol::EventMsg;

        use super::super::plan_mode_stream::PlanModeStreamState;
        use super::super::plan_mode_stream::handle_plan_segments;
        use super::ParsedAssistantTextDelta;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        pub(in crate::praxis::turn_loop_adapter) async fn emit_streamed_assistant_text_delta(
            sess: &Session,
            turn_context: &TurnContext,
            plan_mode_state: Option<&mut PlanModeStreamState>,
            item_id: &str,
            parsed: ParsedAssistantTextDelta,
        ) {
            if parsed.is_empty() {
                return;
            }
            if !parsed.citations.is_empty() {
                let _citations = parsed.citations;
            }
            if let Some(state) = plan_mode_state {
                if !parsed.plan_segments.is_empty() {
                    handle_plan_segments(sess, turn_context, state, item_id, parsed.plan_segments)
                        .await;
                }
                return;
            }
            emit_visible_text_delta(sess, turn_context, item_id, parsed.visible_text).await;
        }

        async fn emit_visible_text_delta(
            sess: &Session,
            turn_context: &TurnContext,
            item_id: &str,
            visible_text: String,
        ) {
            if visible_text.is_empty() {
                return;
            }
            let event = AgentMessageContentDeltaEvent {
                thread_id: sess.conversation_id.to_string(),
                turn_id: turn_context.sub_id.clone(),
                item_id: item_id.to_string(),
                delta: visible_text,
            };
            sess.send_event(turn_context, EventMsg::AgentMessageContentDelta(event))
                .await;
        }
    }
    mod parser_state {
        use std::collections::HashMap;

        use praxis_utils_stream_parser::AssistantTextChunk;
        use praxis_utils_stream_parser::AssistantTextStreamParser;

        #[derive(Debug, Default)]
        pub(in crate::praxis::turn_loop_adapter) struct AssistantMessageStreamParsers {
            plan_mode: bool,
            parsers_by_item: HashMap<String, AssistantTextStreamParser>,
        }

        pub(in crate::praxis::turn_loop_adapter) type ParsedAssistantTextDelta = AssistantTextChunk;

        impl AssistantMessageStreamParsers {
            pub(in crate::praxis::turn_loop_adapter) fn new(plan_mode: bool) -> Self {
                Self {
                    plan_mode,
                    parsers_by_item: HashMap::new(),
                }
            }

            pub(in crate::praxis::turn_loop_adapter) fn seed_item_text(
                &mut self,
                item_id: &str,
                text: &str,
            ) -> ParsedAssistantTextDelta {
                if text.is_empty() {
                    return ParsedAssistantTextDelta::default();
                }
                self.parser_mut(item_id).push_str(text)
            }

            pub(in crate::praxis::turn_loop_adapter) fn parse_delta(
                &mut self,
                item_id: &str,
                delta: &str,
            ) -> ParsedAssistantTextDelta {
                self.parser_mut(item_id).push_str(delta)
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn finish_item(
                &mut self,
                item_id: &str,
            ) -> ParsedAssistantTextDelta {
                let Some(mut parser) = self.parsers_by_item.remove(item_id) else {
                    return ParsedAssistantTextDelta::default();
                };
                parser.finish()
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn drain_finished(
                &mut self,
            ) -> Vec<(String, ParsedAssistantTextDelta)> {
                let parsers_by_item = std::mem::take(&mut self.parsers_by_item);
                parsers_by_item
                    .into_iter()
                    .map(|(item_id, mut parser)| (item_id, parser.finish()))
                    .collect()
            }

            fn parser_mut(&mut self, item_id: &str) -> &mut AssistantTextStreamParser {
                let plan_mode = self.plan_mode;
                self.parsers_by_item
                    .entry(item_id.to_string())
                    .or_insert_with(|| AssistantTextStreamParser::new(plan_mode))
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) use emitter::emit_streamed_assistant_text_delta;
    pub(in crate::praxis::turn_loop_adapter::model_stream) use parser_state::AssistantMessageStreamParsers;
    pub(in crate::praxis::turn_loop_adapter::model_stream) use parser_state::ParsedAssistantTextDelta;

    use super::plan_mode_stream::PlanModeStreamState;

    use crate::praxis::Session;
    use crate::praxis::TurnContext;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn flush_assistant_text_segments_for_item(
        sess: &Session,
        turn_context: &TurnContext,
        plan_mode_state: Option<&mut PlanModeStreamState>,
        parsers: &mut AssistantMessageStreamParsers,
        item_id: &str,
    ) {
        let parsed = parsers.finish_item(item_id);
        emit_streamed_assistant_text_delta(sess, turn_context, plan_mode_state, item_id, parsed)
            .await;
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn flush_assistant_text_segments_all(
        sess: &Session,
        turn_context: &TurnContext,
        mut plan_mode_state: Option<&mut PlanModeStreamState>,
        parsers: &mut AssistantMessageStreamParsers,
    ) {
        for (item_id, parsed) in parsers.drain_finished() {
            emit_streamed_assistant_text_delta(
                sess,
                turn_context,
                plan_mode_state.as_deref_mut(),
                &item_id,
                parsed,
            )
            .await;
        }
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod plan_mode_stream {
    use praxis_protocol::items::TurnItem;
    use praxis_protocol::models::ResponseItem;

    use crate::praxis::Session;
    use crate::praxis::TurnContext;
    use crate::turn_output_items::CompletedResponseItemSink;
    use crate::turn_output_items::handle_non_tool_response_item;

    mod agent_message {
        use praxis_protocol::items::AgentMessageContent;
        use praxis_protocol::items::AgentMessageItem;
        use praxis_protocol::items::TurnItem;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::PlanModeStreamState;

        fn agent_message_text(item: &AgentMessageItem) -> String {
            item.content
                .iter()
                .map(|entry| match entry {
                    AgentMessageContent::Text { text } => text.as_str(),
                })
                .collect()
        }

        async fn emit_agent_message_in_plan_mode(
            sess: &Session,
            turn_context: &TurnContext,
            agent_message: AgentMessageItem,
            state: &mut PlanModeStreamState,
        ) {
            let agent_message_id = agent_message.id.clone();
            let text = agent_message_text(&agent_message);
            if text.trim().is_empty() {
                state.forget_agent_message(&agent_message_id);
                return;
            }

            state
                .emit_pending_agent_message_start(sess, turn_context, &agent_message_id)
                .await;

            if !state.agent_message_started(&agent_message_id) {
                let start_item = state
                    .take_pending_agent_message(&agent_message_id)
                    .unwrap_or_else(|| {
                        TurnItem::AgentMessage(AgentMessageItem {
                            id: agent_message_id.clone(),
                            content: Vec::new(),
                            phase: None,
                            memory_citation: None,
                        })
                    });
                sess.emit_turn_item_started(turn_context, &start_item).await;
                state.mark_agent_message_started(agent_message_id.clone());
            }

            sess.emit_turn_item_completed(turn_context, TurnItem::AgentMessage(agent_message))
                .await;
            state.clear_agent_message_started(&agent_message_id);
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn emit_turn_item_in_plan_mode(
            sess: &Session,
            turn_context: &TurnContext,
            turn_item: TurnItem,
            previously_active_item: Option<&TurnItem>,
            state: &mut PlanModeStreamState,
        ) {
            match turn_item {
                TurnItem::AgentMessage(agent_message) => {
                    emit_agent_message_in_plan_mode(sess, turn_context, agent_message, state).await;
                }
                _ => {
                    if previously_active_item.is_none() {
                        sess.emit_turn_item_started(turn_context, &turn_item).await;
                    }
                    sess.emit_turn_item_completed(turn_context, turn_item).await;
                }
            }
        }
    }
    mod message_completion {
        use praxis_protocol::models::ContentItem;
        use praxis_protocol::models::ResponseItem;
        use praxis_utils_stream_parser::extract_proposed_plan_text;
        use praxis_utils_stream_parser::strip_citations;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::PlanModeStreamState;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn maybe_complete_plan_item_from_message(
            sess: &Session,
            turn_context: &TurnContext,
            state: &mut PlanModeStreamState,
            item: &ResponseItem,
        ) {
            if let ResponseItem::Message { role, content, .. } = item
                && role == "assistant"
            {
                let mut text = String::new();
                for entry in content {
                    if let ContentItem::OutputText { text: chunk } = entry {
                        text.push_str(chunk);
                    }
                }
                if let Some(plan_text) = extract_proposed_plan_text(&text) {
                    let (plan_text, _citations) = strip_citations(&plan_text);
                    if !state.plan_item_started() {
                        state.start_plan_item(sess, turn_context).await;
                    }
                    state
                        .complete_plan_item_with_text(sess, turn_context, plan_text)
                        .await;
                }
            }
        }
    }
    mod plan_item {
        use praxis_protocol::items::PlanItem;
        use praxis_protocol::items::TurnItem;
        use praxis_protocol::protocol::EventMsg;
        use praxis_protocol::protocol::PlanDeltaEvent;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        pub(in crate::praxis::turn_loop_adapter::model_stream) struct ProposedPlanItemState {
            item_id: String,
            pub(in crate::praxis::turn_loop_adapter::model_stream) started: bool,
            pub(in crate::praxis::turn_loop_adapter::model_stream) completed: bool,
        }

        impl ProposedPlanItemState {
            pub(in crate::praxis::turn_loop_adapter::model_stream) fn new(turn_id: &str) -> Self {
                Self {
                    item_id: format!("{turn_id}-plan"),
                    started: false,
                    completed: false,
                }
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn start(
                &mut self,
                sess: &Session,
                turn_context: &TurnContext,
            ) {
                if self.started || self.completed {
                    return;
                }
                self.started = true;
                let item = TurnItem::Plan(PlanItem {
                    id: self.item_id.clone(),
                    text: String::new(),
                });
                sess.emit_turn_item_started(turn_context, &item).await;
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn push_delta(
                &mut self,
                sess: &Session,
                turn_context: &TurnContext,
                delta: &str,
            ) {
                if self.completed || delta.is_empty() {
                    return;
                }
                let event = PlanDeltaEvent {
                    thread_id: sess.conversation_id.to_string(),
                    turn_id: turn_context.sub_id.clone(),
                    item_id: self.item_id.clone(),
                    delta: delta.to_string(),
                };
                sess.send_event(turn_context, EventMsg::PlanDelta(event))
                    .await;
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn complete_with_text(
                &mut self,
                sess: &Session,
                turn_context: &TurnContext,
                text: String,
            ) {
                if self.completed || !self.started {
                    return;
                }
                self.completed = true;
                let item = TurnItem::Plan(PlanItem {
                    id: self.item_id.clone(),
                    text,
                });
                sess.emit_turn_item_completed(turn_context, item).await;
            }
        }
    }
    mod segments {
        use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
        use praxis_protocol::protocol::EventMsg;
        use praxis_utils_stream_parser::ProposedPlanSegment;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::PlanModeStreamState;

        pub(in crate::praxis::turn_loop_adapter) async fn handle_plan_segments(
            sess: &Session,
            turn_context: &TurnContext,
            state: &mut PlanModeStreamState,
            item_id: &str,
            segments: Vec<ProposedPlanSegment>,
        ) {
            for segment in segments {
                match segment {
                    ProposedPlanSegment::Normal(delta) => {
                        handle_normal_text_delta(sess, turn_context, state, item_id, delta).await;
                    }
                    ProposedPlanSegment::ProposedPlanStart => {
                        if !state.plan_item_completed() {
                            state.start_plan_item(sess, turn_context).await;
                        }
                    }
                    ProposedPlanSegment::ProposedPlanDelta(delta) => {
                        if !state.plan_item_completed() {
                            if !state.plan_item_started() {
                                state.start_plan_item(sess, turn_context).await;
                            }
                            state.push_plan_delta(sess, turn_context, &delta).await;
                        }
                    }
                    ProposedPlanSegment::ProposedPlanEnd => {}
                }
            }
        }

        async fn handle_normal_text_delta(
            sess: &Session,
            turn_context: &TurnContext,
            state: &mut PlanModeStreamState,
            item_id: &str,
            delta: String,
        ) {
            if delta.is_empty() {
                return;
            }
            let has_non_whitespace = delta.chars().any(|ch| !ch.is_whitespace());
            if !has_non_whitespace && !state.agent_message_started(item_id) {
                state.push_leading_whitespace(item_id, &delta);
                return;
            }
            let delta = if !state.agent_message_started(item_id) {
                if let Some(prefix) = state.take_leading_whitespace(item_id) {
                    format!("{prefix}{delta}")
                } else {
                    delta
                }
            } else {
                delta
            };
            state
                .emit_pending_agent_message_start(sess, turn_context, item_id)
                .await;

            let event = AgentMessageContentDeltaEvent {
                thread_id: sess.conversation_id.to_string(),
                turn_id: turn_context.sub_id.clone(),
                item_id: item_id.to_string(),
                delta,
            };
            sess.send_event(turn_context, EventMsg::AgentMessageContentDelta(event))
                .await;
        }
    }
    mod state {
        use std::collections::HashMap;
        use std::collections::HashSet;

        use praxis_protocol::items::TurnItem;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::plan_item::ProposedPlanItemState;

        pub(in crate::praxis::turn_loop_adapter) struct PlanModeStreamState {
            pending_agent_message_items: HashMap<String, TurnItem>,
            started_agent_message_items: HashSet<String>,
            leading_whitespace_by_item: HashMap<String, String>,
            plan_item_state: ProposedPlanItemState,
        }

        impl PlanModeStreamState {
            pub(in crate::praxis::turn_loop_adapter) fn new(turn_id: &str) -> Self {
                Self {
                    pending_agent_message_items: HashMap::new(),
                    started_agent_message_items: HashSet::new(),
                    leading_whitespace_by_item: HashMap::new(),
                    plan_item_state: ProposedPlanItemState::new(turn_id),
                }
            }

            pub(in crate::praxis::turn_loop_adapter) fn insert_pending_agent_message(
                &mut self,
                item_id: String,
                item: TurnItem,
            ) {
                self.pending_agent_message_items.insert(item_id, item);
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn forget_agent_message(
                &mut self,
                item_id: &str,
            ) {
                self.pending_agent_message_items.remove(item_id);
                self.started_agent_message_items.remove(item_id);
                self.leading_whitespace_by_item.remove(item_id);
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn agent_message_started(
                &self,
                item_id: &str,
            ) -> bool {
                self.started_agent_message_items.contains(item_id)
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn mark_agent_message_started(
                &mut self,
                item_id: impl Into<String>,
            ) {
                self.started_agent_message_items.insert(item_id.into());
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn clear_agent_message_started(
                &mut self,
                item_id: &str,
            ) {
                self.started_agent_message_items.remove(item_id);
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn take_pending_agent_message(
                &mut self,
                item_id: &str,
            ) -> Option<TurnItem> {
                self.pending_agent_message_items.remove(item_id)
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn push_leading_whitespace(
                &mut self,
                item_id: &str,
                delta: &str,
            ) {
                self.leading_whitespace_by_item
                    .entry(item_id.to_string())
                    .or_default()
                    .push_str(delta);
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn take_leading_whitespace(
                &mut self,
                item_id: &str,
            ) -> Option<String> {
                self.leading_whitespace_by_item.remove(item_id)
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn emit_pending_agent_message_start(
                &mut self,
                sess: &Session,
                turn_context: &TurnContext,
                item_id: &str,
            ) {
                if self.agent_message_started(item_id) {
                    return;
                }
                if let Some(item) = self.take_pending_agent_message(item_id) {
                    sess.emit_turn_item_started(turn_context, &item).await;
                    self.mark_agent_message_started(item_id.to_string());
                }
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn plan_item_started(
                &self,
            ) -> bool {
                self.plan_item_state.started
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn plan_item_completed(
                &self,
            ) -> bool {
                self.plan_item_state.completed
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn start_plan_item(
                &mut self,
                sess: &Session,
                turn_context: &TurnContext,
            ) {
                self.plan_item_state.start(sess, turn_context).await;
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn push_plan_delta(
                &mut self,
                sess: &Session,
                turn_context: &TurnContext,
                delta: &str,
            ) {
                self.plan_item_state
                    .push_delta(sess, turn_context, delta)
                    .await;
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn complete_plan_item_with_text(
                &mut self,
                sess: &Session,
                turn_context: &TurnContext,
                text: String,
            ) {
                self.plan_item_state
                    .complete_with_text(sess, turn_context, text)
                    .await;
            }
        }
    }

    use agent_message::emit_turn_item_in_plan_mode;
    use message_completion::maybe_complete_plan_item_from_message;
    pub(in crate::praxis::turn_loop_adapter::model_stream) use segments::handle_plan_segments;
    pub(in crate::praxis::turn_loop_adapter::model_stream) use state::PlanModeStreamState;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_assistant_item_done_in_plan_mode(
        sess: &Session,
        turn_context: &TurnContext,
        item: &ResponseItem,
        state: &mut PlanModeStreamState,
        previously_active_item: Option<&TurnItem>,
        last_agent_message: &mut Option<String>,
    ) -> bool {
        if let ResponseItem::Message { role, .. } = item
            && role == "assistant"
        {
            maybe_complete_plan_item_from_message(sess, turn_context, state, item).await;

            if let Some(turn_item) =
                handle_non_tool_response_item(sess, turn_context, item, /*plan_mode*/ true).await
            {
                emit_turn_item_in_plan_mode(
                    sess,
                    turn_context,
                    turn_item,
                    previously_active_item,
                    state,
                )
                .await;
            }

            let sink = CompletedResponseItemSink::new(sess, turn_context);
            if let Some(agent_message) = sink.record_completed(item).await {
                *last_agent_message = Some(agent_message);
            }
            return true;
        }
        false
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod reasoning_delta_stream {
    use std::sync::Arc;

    use praxis_protocol::items::TurnItem;
    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::ReasoningContentDeltaEvent;
    use praxis_protocol::protocol::ReasoningRawContentDeltaEvent;

    use crate::praxis::Session;
    use crate::praxis::TurnContext;
    use crate::util::error_or_panic;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn emit_reasoning_summary_delta(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        active_item: Option<&TurnItem>,
        delta: String,
        summary_index: i64,
    ) {
        if let Some(active) = active_item {
            let event = ReasoningContentDeltaEvent {
                thread_id: sess.conversation_id.to_string(),
                turn_id: turn_context.sub_id.clone(),
                item_id: active.id(),
                delta,
                summary_index,
            };
            sess.send_event(turn_context, EventMsg::ReasoningContentDelta(event))
                .await;
        } else {
            error_or_panic("ReasoningSummaryDelta without active item".to_string());
        }
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn emit_reasoning_content_delta(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        active_item: Option<&TurnItem>,
        delta: String,
        content_index: i64,
    ) {
        if let Some(active) = active_item {
            let event = ReasoningRawContentDeltaEvent {
                thread_id: sess.conversation_id.to_string(),
                turn_id: turn_context.sub_id.clone(),
                item_id: active.id(),
                delta,
                content_index,
            };
            sess.send_event(turn_context, EventMsg::ReasoningRawContentDelta(event))
                .await;
        } else {
            error_or_panic("ReasoningRawContentDelta without active item".to_string());
        }
    }
}

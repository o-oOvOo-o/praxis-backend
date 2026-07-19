//! Response item identity, lifecycle, delta, and completion handling.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter::model_stream) mod stream_item_completion {
    use super::assistant_text_stream::AssistantMessageStreamParsers;
    use super::assistant_text_stream::flush_assistant_text_segments_for_item;
    use super::plan_mode_stream::PlanModeStreamState;
    use super::plan_mode_stream::handle_assistant_item_done_in_plan_mode;
    use std::sync::Arc;

    use praxis_protocol::items::TurnItem;
    use praxis_protocol::models::ResponseItem;

    use crate::error::Result as PraxisResult;
    use crate::praxis::Session;
    use crate::praxis::TurnContext;
    use crate::turn_output_items::CompletedResponseItemSink;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn complete_non_tool_output_item(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        active_item: &mut Option<TurnItem>,
        last_agent_message: &mut Option<String>,
        mut plan_mode_state: Option<&mut PlanModeStreamState>,
        assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
        item: ResponseItem,
    ) -> PraxisResult<Option<String>> {
        let previously_active_item = active_item.take();
        flush_previous_assistant_item(
            sess,
            turn_context,
            previously_active_item.as_ref(),
            plan_mode_state.as_deref_mut(),
            assistant_message_stream_parsers,
        )
        .await;

        if let Some(state) = plan_mode_state.as_deref_mut()
            && handle_assistant_item_done_in_plan_mode(
                sess,
                turn_context,
                &item,
                state,
                previously_active_item.as_ref(),
                last_agent_message,
            )
            .await
        {
            return Ok(last_agent_message.clone());
        }

        let completed_message =
            emit_completed_non_tool_item(sess, turn_context, &item, &previously_active_item).await;
        if let Some(agent_message) = completed_message.as_ref() {
            *last_agent_message = Some(agent_message.clone());
        }
        Ok(completed_message)
    }

    async fn flush_previous_assistant_item(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        previous_item: Option<&TurnItem>,
        plan_mode_state: Option<&mut PlanModeStreamState>,
        assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
    ) {
        let Some(previous) = previous_item else {
            return;
        };
        if !matches!(previous, TurnItem::AgentMessage(_)) {
            return;
        }
        let item_id = previous.id();
        flush_assistant_text_segments_for_item(
            sess,
            turn_context,
            plan_mode_state,
            assistant_message_stream_parsers,
            &item_id,
        )
        .await;
    }

    async fn emit_completed_non_tool_item(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        item: &ResponseItem,
        previous_item: &Option<TurnItem>,
    ) -> Option<String> {
        let sink = CompletedResponseItemSink::new(sess.as_ref(), turn_context.as_ref());
        sink.emit_and_record(item, previous_item.as_ref()).await
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod stream_item_delta {
    use super::assistant_text_stream::AssistantMessageStreamParsers;
    use super::assistant_text_stream::emit_streamed_assistant_text_delta;
    use super::plan_mode_stream::PlanModeStreamState;
    use std::sync::Arc;

    use praxis_protocol::items::TurnItem;
    use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
    use praxis_protocol::protocol::EventMsg;

    use crate::praxis::Session;
    use crate::praxis::TurnContext;
    use crate::util::error_or_panic;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn emit_output_text_delta(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        active_item: Option<&TurnItem>,
        plan_mode_state: Option<&mut PlanModeStreamState>,
        assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
        delta: String,
    ) {
        let Some(active) = active_item else {
            error_or_panic("OutputTextDelta without active item".to_string());
            return;
        };

        let item_id = active.id();
        if matches!(active, TurnItem::AgentMessage(_)) {
            let parsed = assistant_message_stream_parsers.parse_delta(&item_id, &delta);
            emit_streamed_assistant_text_delta(
                sess,
                turn_context,
                plan_mode_state,
                &item_id,
                parsed,
            )
            .await;
            return;
        }

        let event = AgentMessageContentDeltaEvent {
            thread_id: sess.conversation_id.to_string(),
            turn_id: turn_context.sub_id.clone(),
            item_id,
            delta,
        };
        sess.send_event(turn_context, EventMsg::AgentMessageContentDelta(event))
            .await;
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod stream_item_start {
    use std::sync::Arc;

    use praxis_protocol::items::TurnItem;
    use praxis_protocol::models::ResponseItem;

    use crate::praxis::Session;
    use crate::praxis::TurnContext;
    use crate::turn_output_items::handle_non_tool_response_item;

    use super::assistant_text_stream::AssistantMessageStreamParsers;
    use super::assistant_text_stream::emit_streamed_assistant_text_delta;
    use super::plan_mode_stream::PlanModeStreamState;

    mod assistant_seed {
        use praxis_protocol::items::AgentMessageContent;
        use praxis_protocol::items::TurnItem;
        use praxis_protocol::models::ResponseItem;

        use crate::turn_assistant_text::raw_assistant_output_text_from_item;

        use super::super::assistant_text_stream::AssistantMessageStreamParsers;
        use super::super::assistant_text_stream::ParsedAssistantTextDelta;

        pub(in crate::praxis::turn_loop_adapter::model_stream) struct StartedAssistantSeed {
            pub(in crate::praxis::turn_loop_adapter::model_stream) item_id: String,
            pub(in crate::praxis::turn_loop_adapter::model_stream) parsed: ParsedAssistantTextDelta,
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn seed_assistant_text(
            turn_item: &mut TurnItem,
            response_item: &ResponseItem,
            plan_mode: bool,
            parsers: &mut AssistantMessageStreamParsers,
        ) -> Option<StartedAssistantSeed> {
            if !matches!(turn_item, TurnItem::AgentMessage(_)) {
                return None;
            }

            let raw_text = raw_assistant_output_text_from_item(response_item)?;
            let item_id = turn_item.id();
            let mut seeded = parsers.seed_item_text(&item_id, &raw_text);

            if let TurnItem::AgentMessage(agent_message) = turn_item {
                agent_message.content = vec![AgentMessageContent::Text {
                    text: if plan_mode {
                        String::new()
                    } else {
                        std::mem::take(&mut seeded.visible_text)
                    },
                }];
            }

            plan_mode.then_some(StartedAssistantSeed {
                item_id,
                parsed: seeded,
            })
        }
    }
    mod emit_started {
        use std::sync::Arc;

        use praxis_protocol::items::TurnItem;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::super::plan_mode_stream::PlanModeStreamState;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn emit_or_queue_started_item(
            sess: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            turn_item: &TurnItem,
            plan_mode_state: Option<&mut PlanModeStreamState>,
        ) {
            if let Some(state) = plan_mode_state
                && matches!(turn_item, TurnItem::AgentMessage(_))
            {
                state.insert_pending_agent_message(turn_item.id(), turn_item.clone());
                return;
            }

            sess.emit_turn_item_started(turn_context, turn_item).await;
        }
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn start_stream_item(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        item: ResponseItem,
        plan_mode: bool,
        mut plan_mode_state: Option<&mut PlanModeStreamState>,
        assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
    ) -> Option<TurnItem> {
        let mut turn_item =
            handle_non_tool_response_item(sess.as_ref(), turn_context.as_ref(), &item, plan_mode)
                .await?;

        let seeded = assistant_seed::seed_assistant_text(
            &mut turn_item,
            &item,
            plan_mode,
            assistant_message_stream_parsers,
        );
        emit_started::emit_or_queue_started_item(
            sess,
            turn_context,
            &turn_item,
            plan_mode_state.as_deref_mut(),
        )
        .await;

        if let (Some(state), Some(seed)) = (plan_mode_state.as_deref_mut(), seeded) {
            emit_streamed_assistant_text_delta(
                sess,
                turn_context,
                Some(state),
                &seed.item_id,
                seed.parsed,
            )
            .await;
        }

        Some(turn_item)
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod stream_item_state {
    use super::assistant_text_stream::AssistantMessageStreamParsers;
    use super::plan_mode_stream::PlanModeStreamState;
    use praxis_protocol::config_types::ModeKind;
    use praxis_protocol::items::TurnItem;

    use crate::praxis::TurnContext;

    mod completion {
        use std::sync::Arc;

        use praxis_protocol::models::ResponseItem;

        use crate::error::Result as PraxisResult;
        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::super::stream_item_completion::complete_non_tool_output_item;
        use super::StreamItemState;

        impl StreamItemState {
            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_completed_non_tool_output_item(
                &mut self,
                sess: &Arc<Session>,
                turn_context: &Arc<TurnContext>,
                item: ResponseItem,
            ) -> PraxisResult<Option<String>> {
                complete_non_tool_output_item(
                    sess,
                    turn_context,
                    &mut self.active_item,
                    &mut self.last_agent_message,
                    self.plan_mode_state.as_mut(),
                    &mut self.assistant_message_stream_parsers,
                    item,
                )
                .await
            }
        }
    }
    mod delta {
        use std::sync::Arc;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::super::stream_item_delta::emit_output_text_delta;
        use super::StreamItemState;

        impl StreamItemState {
            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_output_text_delta(
                &mut self,
                sess: &Arc<Session>,
                turn_context: &Arc<TurnContext>,
                delta: String,
            ) {
                emit_output_text_delta(
                    sess,
                    turn_context,
                    self.active_item.as_ref(),
                    self.plan_mode_state.as_mut(),
                    &mut self.assistant_message_stream_parsers,
                    delta,
                )
                .await;
            }
        }
    }
    mod flush {
        use std::sync::Arc;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::super::assistant_text_stream::flush_assistant_text_segments_all;
        use super::StreamItemState;

        impl StreamItemState {
            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn flush_assistant_text(
                &mut self,
                sess: &Arc<Session>,
                turn_context: &Arc<TurnContext>,
            ) {
                flush_assistant_text_segments_all(
                    sess,
                    turn_context,
                    self.plan_mode_state.as_mut(),
                    &mut self.assistant_message_stream_parsers,
                )
                .await;
            }
        }
    }
    mod reasoning {
        use std::sync::Arc;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::super::reasoning_delta_stream::emit_reasoning_content_delta;
        use super::super::reasoning_delta_stream::emit_reasoning_summary_delta;
        use super::StreamItemState;

        impl StreamItemState {
            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_reasoning_summary_delta(
                &mut self,
                sess: &Arc<Session>,
                turn_context: &Arc<TurnContext>,
                delta: String,
                summary_index: i64,
            ) {
                emit_reasoning_summary_delta(
                    sess,
                    turn_context,
                    self.active_item.as_ref(),
                    delta,
                    summary_index,
                )
                .await;
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_reasoning_content_delta(
                &mut self,
                sess: &Arc<Session>,
                turn_context: &Arc<TurnContext>,
                delta: String,
                content_index: i64,
            ) {
                emit_reasoning_content_delta(
                    sess,
                    turn_context,
                    self.active_item.as_ref(),
                    delta,
                    content_index,
                )
                .await;
            }
        }
    }
    mod start {
        use std::sync::Arc;

        use praxis_protocol::models::ResponseItem;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;

        use super::super::stream_item_start::start_stream_item;
        use super::StreamItemState;

        impl StreamItemState {
            pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_output_item_added(
                &mut self,
                sess: &Arc<Session>,
                turn_context: &Arc<TurnContext>,
                item: ResponseItem,
            ) {
                if let Some(turn_item) = start_stream_item(
                    sess,
                    turn_context,
                    item,
                    self.plan_mode,
                    self.plan_mode_state.as_mut(),
                    &mut self.assistant_message_stream_parsers,
                )
                .await
                {
                    self.active_item = Some(turn_item);
                }
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) struct StreamItemState {
        active_item: Option<TurnItem>,
        last_agent_message: Option<String>,
        plan_mode: bool,
        assistant_message_stream_parsers: AssistantMessageStreamParsers,
        plan_mode_state: Option<PlanModeStreamState>,
    }

    impl StreamItemState {
        pub(in crate::praxis::turn_loop_adapter::model_stream) fn new(
            turn_context: &TurnContext,
        ) -> Self {
            let plan_mode = turn_context.collaboration_mode.mode == ModeKind::Plan;
            Self {
                active_item: None,
                last_agent_message: None,
                plan_mode,
                assistant_message_stream_parsers: AssistantMessageStreamParsers::new(plan_mode),
                plan_mode_state: plan_mode.then(|| PlanModeStreamState::new(&turn_context.sub_id)),
            }
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn active_item_id(
            &self,
        ) -> Option<String> {
            self.active_item.as_ref().map(TurnItem::id)
        }
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod response_item_identity {
    use praxis_protocol::models::ResponseItem;

    pub(in crate::praxis::turn_loop_adapter::model_stream) fn response_item_id(
        item: &ResponseItem,
    ) -> Option<String> {
        match item {
            ResponseItem::Message { id, .. } => id.clone(),
            ResponseItem::Reasoning { id, .. } => Some(id.clone()),
            ResponseItem::FunctionCall { call_id, .. }
            | ResponseItem::FunctionCallOutput { call_id, .. }
            | ResponseItem::CustomToolCall { call_id, .. }
            | ResponseItem::CustomToolCallOutput { call_id, .. } => Some(call_id.clone()),
            ResponseItem::LocalShellCall { call_id, id, .. }
            | ResponseItem::ToolSearchCall { call_id, id, .. } => {
                call_id.clone().or_else(|| id.clone())
            }
            ResponseItem::ToolSearchOutput { call_id, .. } => call_id.clone(),
            ResponseItem::WebSearchCall { id, .. } => id.clone(),
            ResponseItem::ImageGenerationCall { id, .. } => Some(id.clone()),
            ResponseItem::WorkspaceCheckpoint { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::Other => None,
        }
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod non_tool_item {
    use praxis_loop::model::ModelEvent;
    use praxis_loop::outcome::LoopResult;
    use praxis_protocol::models::ResponseItem;

    use super::stream_item_state::StreamItemState;

    use super::PraxisModelStreamInput;
    use super::error_bridge::model_error;
    use super::provider_projection::ModelOutputObservation;
    use super::provider_projection::ProviderEventProjection;
    use super::response_item_identity::response_item_id;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn record_completed_non_tool_item(
        input: &PraxisModelStreamInput,
        stream_items: &mut StreamItemState,
        item: ResponseItem,
    ) -> LoopResult<ProviderEventProjection> {
        let item_id = response_item_id(&item);
        let Some(message) = stream_items
            .handle_completed_non_tool_output_item(&input.session, &input.turn_context, item)
            .await
            .map_err(model_error)?
        else {
            return Ok(ProviderEventProjection::ignore(
                ModelOutputObservation::Observed,
            ));
        };
        input
            .bridge_state
            .record_agent_message(message.clone())
            .await;
        Ok(ProviderEventProjection::loop_event(
            ModelEvent::RecordedFinalText {
                item_id,
                text: message,
            },
        ))
    }
}

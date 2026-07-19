//! Provider event projection into canonical turn-loop effects.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter::model_stream) mod provider_projection {
    use praxis_loop::outcome::LoopResult;

    use super::stream_item_state::StreamItemState;
    use crate::client_common::ResponseEvent;

    pub(in crate::praxis::turn_loop_adapter::model_stream) use self::event::ModelOutputObservation;
    pub(in crate::praxis::turn_loop_adapter::model_stream) use self::event::ProviderEventProjection;
    pub(in crate::praxis::turn_loop_adapter::model_stream) use self::stream_step::ProviderStreamStep;
    use super::PraxisModelStreamInput;

    mod completion {
        use praxis_loop::model::ModelEvent;
        use praxis_loop::model::TokenUsage as LoopTokenUsage;
        use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

        use super::super::stream_item_state::StreamItemState;

        use super::super::PraxisModelStreamInput;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn finish_completed_stream(
            input: &PraxisModelStreamInput,
            stream_items: &mut StreamItemState,
            protocol_usage: Option<ProtocolTokenUsage>,
            loop_usage: LoopTokenUsage,
        ) -> ModelEvent {
            stream_items
                .flush_assistant_text(&input.session, &input.turn_context)
                .await;
            input
                .session
                .update_token_usage_info(&input.turn_context, protocol_usage.as_ref())
                .await;
            ModelEvent::Completed(loop_usage)
        }
    }
    mod effect {
        use std::sync::Arc;

        use praxis_protocol::protocol::AgentReasoningSectionBreakEvent;
        use praxis_protocol::protocol::EventMsg;
        use praxis_protocol::protocol::RateLimitSnapshot;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;
        use crate::util::error_or_panic;

        #[derive(Debug)]
        pub(in crate::praxis::turn_loop_adapter::model_stream) enum ProviderEffect {
            ServerModel(String),
            ServerReasoningIncluded(bool),
            RateLimits(RateLimitSnapshot),
            ModelsEtag(String),
            ReasoningSummaryPartAdded {
                item_id: Option<String>,
                summary_index: i64,
            },
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn apply_provider_effect(
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
    }
    mod effect_application {
        use super::effect::ProviderEffect;
        use super::effect::apply_provider_effect;

        use super::super::PraxisModelStreamInput;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn apply_core_effect(
            input: &PraxisModelStreamInput,
            effect: ProviderEffect,
        ) {
            let mut runtime_state = input.runtime_state.lock().await;
            apply_provider_effect(
                &input.session,
                &input.turn_context,
                effect,
                runtime_state.server_model_warning_emitted_for_turn_mut(),
            )
            .await;
        }
    }
    mod event {
        use praxis_loop::model::ModelEvent;
        use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

        use super::effect::ProviderEffect;

        use super::super::token_usage_bridge;

        pub(in crate::praxis::turn_loop_adapter::model_stream) enum ProviderEventProjection {
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
        pub(in crate::praxis::turn_loop_adapter::model_stream) enum ModelOutputObservation {
            Observed,
            NotObserved,
        }

        impl ModelOutputObservation {
            pub(in crate::praxis::turn_loop_adapter::model_stream) const fn as_bool(self) -> bool {
                matches!(self, Self::Observed)
            }
        }

        impl ProviderEventProjection {
            pub(in crate::praxis::turn_loop_adapter::model_stream) fn loop_event(
                event: ModelEvent,
            ) -> Self {
                Self::Loop(event)
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn completed(
                protocol_usage: Option<ProtocolTokenUsage>,
            ) -> Self {
                let loop_usage = token_usage_bridge::protocol_to_loop(protocol_usage.as_ref());
                Self::Completed {
                    protocol_usage,
                    loop_usage,
                }
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn core_effect(
                effect: ProviderEffect,
                observation: ModelOutputObservation,
            ) -> Self {
                Self::CoreEffect {
                    effect,
                    observation,
                }
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn ignore(
                observation: ModelOutputObservation,
            ) -> Self {
                Self::Ignore { observation }
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn observed_model_output(
                &self,
            ) -> ModelOutputObservation {
                match self {
                    ProviderEventProjection::Loop(_)
                    | ProviderEventProjection::Completed { .. } => ModelOutputObservation::Observed,
                    ProviderEventProjection::CoreEffect { observation, .. }
                    | ProviderEventProjection::Ignore { observation } => *observation,
                }
            }
        }
    }
    mod event_handler {
        use praxis_loop::outcome::LoopResult;

        use super::super::stream_item_state::StreamItemState;
        use super::response_event::ModelResponseEvent;
        use super::response_event::classify_response_event;

        use super::super::PraxisModelStreamInput;
        use super::super::item_completion::handle_completed_provider_item;
        use super::event::ProviderEventProjection;
        use super::incremental;
        use super::terminal;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_provider_event(
            input: &PraxisModelStreamInput,
            stream_items: &mut StreamItemState,
            event: crate::client_common::ResponseEvent,
        ) -> LoopResult<ProviderEventProjection> {
            match classify_response_event(event) {
                ModelResponseEvent::ItemAdded(item) => {
                    Ok(incremental::record_item_added(input, stream_items, item).await)
                }
                ModelResponseEvent::TextDelta(delta) => {
                    Ok(incremental::record_text_delta(input, stream_items, delta).await)
                }
                ModelResponseEvent::ReasoningSummaryDelta {
                    delta,
                    summary_index,
                } => Ok(incremental::record_reasoning_summary_delta(
                    input,
                    stream_items,
                    delta,
                    summary_index,
                )
                .await),
                ModelResponseEvent::ReasoningContentDelta {
                    delta,
                    content_index,
                } => Ok(incremental::record_reasoning_content_delta(
                    input,
                    stream_items,
                    delta,
                    content_index,
                )
                .await),
                ModelResponseEvent::ReasoningSummaryPartAdded { summary_index } => Ok(
                    incremental::reasoning_summary_part_added(stream_items, summary_index),
                ),
                ModelResponseEvent::ItemDone(item) => {
                    handle_completed_provider_item(input, stream_items, item).await
                }
                ModelResponseEvent::Completed { token_usage } => {
                    Ok(terminal::completed(token_usage))
                }
                ModelResponseEvent::Effect(effect) => Ok(terminal::effect(effect)),
                ModelResponseEvent::Ignore => Ok(terminal::ignore()),
            }
        }
    }
    mod incremental {
        use praxis_protocol::models::ResponseItem;

        use super::super::PraxisModelStreamInput;
        use super::super::stream_item_state::StreamItemState;
        use super::effect::ProviderEffect;
        use super::event::ModelOutputObservation;
        use super::event::ProviderEventProjection;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn record_item_added(
            input: &PraxisModelStreamInput,
            stream_items: &mut StreamItemState,
            item: ResponseItem,
        ) -> ProviderEventProjection {
            stream_items
                .handle_output_item_added(&input.session, &input.turn_context, item)
                .await;
            ProviderEventProjection::ignore(ModelOutputObservation::Observed)
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn record_text_delta(
            input: &PraxisModelStreamInput,
            stream_items: &mut StreamItemState,
            delta: String,
        ) -> ProviderEventProjection {
            stream_items
                .handle_output_text_delta(&input.session, &input.turn_context, delta)
                .await;
            ProviderEventProjection::ignore(ModelOutputObservation::Observed)
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn record_reasoning_summary_delta(
            input: &PraxisModelStreamInput,
            stream_items: &mut StreamItemState,
            delta: String,
            summary_index: i64,
        ) -> ProviderEventProjection {
            stream_items
                .handle_reasoning_summary_delta(
                    &input.session,
                    &input.turn_context,
                    delta,
                    summary_index,
                )
                .await;
            ProviderEventProjection::ignore(ModelOutputObservation::Observed)
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn record_reasoning_content_delta(
            input: &PraxisModelStreamInput,
            stream_items: &mut StreamItemState,
            delta: String,
            content_index: i64,
        ) -> ProviderEventProjection {
            stream_items
                .handle_reasoning_content_delta(
                    &input.session,
                    &input.turn_context,
                    delta,
                    content_index,
                )
                .await;
            ProviderEventProjection::ignore(ModelOutputObservation::Observed)
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn reasoning_summary_part_added(
            stream_items: &StreamItemState,
            summary_index: i64,
        ) -> ProviderEventProjection {
            ProviderEventProjection::core_effect(
                ProviderEffect::ReasoningSummaryPartAdded {
                    item_id: stream_items.active_item_id(),
                    summary_index,
                },
                ModelOutputObservation::Observed,
            )
        }
    }
    mod response_event {
        use super::effect::ProviderEffect;
        use crate::client_common::ResponseEvent;
        use praxis_protocol::models::ResponseItem;
        use praxis_protocol::protocol::TokenUsage;

        pub(in crate::praxis::turn_loop_adapter::model_stream) enum ModelResponseEvent {
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

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn classify_response_event(
            event: ResponseEvent,
        ) -> ModelResponseEvent {
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
    }
    mod stream_step {
        use praxis_loop::model::ModelEvent;

        use super::super::stream_item_state::StreamItemState;

        use super::super::PraxisModelStreamInput;
        use super::completion;
        use super::effect_application;
        use super::event::ProviderEventProjection;

        pub(in crate::praxis::turn_loop_adapter::model_stream) enum ProviderStreamStep {
            Yield(ModelEvent),
            Finish(ModelEvent),
            Continue,
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn apply_provider_projection(
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
    }
    mod terminal {
        use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

        use super::effect::ProviderEffect;
        use super::event::ModelOutputObservation;
        use super::event::ProviderEventProjection;

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn completed(
            token_usage: Option<ProtocolTokenUsage>,
        ) -> ProviderEventProjection {
            ProviderEventProjection::completed(token_usage)
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn effect(
            effect: ProviderEffect,
        ) -> ProviderEventProjection {
            ProviderEventProjection::core_effect(effect, ModelOutputObservation::NotObserved)
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) fn ignore() -> ProviderEventProjection
        {
            ProviderEventProjection::ignore(ModelOutputObservation::NotObserved)
        }
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) struct ProjectedProviderEvent {
        pub(in crate::praxis::turn_loop_adapter::model_stream) step: ProviderStreamStep,
        pub(in crate::praxis::turn_loop_adapter::model_stream) observed_model_output:
            ModelOutputObservation,
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn project_response_event(
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
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod item_completion {
    use praxis_loop::outcome::LoopResult;
    use praxis_protocol::models::ResponseItem;

    use super::stream_item_state::StreamItemState;

    use super::PraxisModelStreamInput;
    use super::completed_tool_call;
    use super::completed_tool_call::CompletedItemProjection;
    use super::non_tool_item::record_completed_non_tool_item;
    use super::provider_projection::ProviderEventProjection;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn handle_completed_provider_item(
        input: &PraxisModelStreamInput,
        stream_items: &mut StreamItemState,
        item: ResponseItem,
    ) -> LoopResult<ProviderEventProjection> {
        match completed_tool_call::try_project_completed_tool_call(input, stream_items, &item)
            .await?
        {
            CompletedItemProjection::Projected(projection) => Ok(projection),
            CompletedItemProjection::NonTool => {
                record_completed_non_tool_item(input, stream_items, item).await
            }
        }
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod completed_tool_call {
    use std::sync::Arc;

    use praxis_loop::model::ModelEvent;
    use praxis_loop::outcome::LoopResult;
    use praxis_protocol::models::ResponseItem;
    use tracing::warn;

    use super::stream_item_state::StreamItemState;
    use crate::turn_final_answer::tool_loop_guard_final_item;

    use super::PraxisModelStreamInput;
    use super::completed_tool_call_conversion;
    use super::completed_tool_call_conversion::CompletedToolCallConversion;
    use super::non_tool_item::record_completed_non_tool_item;
    use super::provider_projection::ProviderEventProjection;

    pub(in crate::praxis::turn_loop_adapter::model_stream) enum CompletedItemProjection {
        Projected(ProviderEventProjection),
        NonTool,
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn try_project_completed_tool_call(
        input: &PraxisModelStreamInput,
        stream_items: &mut StreamItemState,
        item: &ResponseItem,
    ) -> LoopResult<CompletedItemProjection> {
        let call =
            match completed_tool_call_conversion::convert_completed_tool_call(input, item).await? {
                CompletedToolCallConversion::ToolCall(call) => call,
                CompletedToolCallConversion::FollowupRequired => {
                    return Ok(CompletedItemProjection::Projected(
                        ProviderEventProjection::loop_event(ModelEvent::FollowupRequired),
                    ));
                }
                CompletedToolCallConversion::NotToolCall => {
                    return Ok(CompletedItemProjection::NonTool);
                }
            };

        if input
            .turn_context
            .tool_loop_guard
            .should_hide_tool(&call.name)
        {
            warn!(
                tool_name = call.name.as_str(),
                "hidden tool call suppressed after tool loop guard intervention"
            );
            let final_item =
                tool_loop_guard_final_item(Arc::clone(&input.session), call.name.as_str()).await;
            return record_completed_non_tool_item(input, stream_items, final_item)
                .await
                .map(CompletedItemProjection::Projected);
        }

        tracing::info!(
            thread_id = %input.session.conversation_id,
            "ToolCall: {} {}",
            call.name,
            call.arguments
        );
        Ok(CompletedItemProjection::Projected(
            ProviderEventProjection::loop_event(ModelEvent::ToolCall(call)),
        ))
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod completed_tool_call_conversion {
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::tool::ToolCall;
    use praxis_protocol::models::ResponseItem;

    use super::super::tool_call_bridge::ResponseItemToolCall;
    use super::super::tool_call_bridge::response_item_to_loop_tool_call;
    use super::PraxisModelStreamInput;
    use super::function_call_error_projection;

    pub(in crate::praxis::turn_loop_adapter::model_stream) enum CompletedToolCallConversion {
        ToolCall(ToolCall),
        FollowupRequired,
        NotToolCall,
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn convert_completed_tool_call(
        input: &PraxisModelStreamInput,
        item: &ResponseItem,
    ) -> LoopResult<CompletedToolCallConversion> {
        match response_item_to_loop_tool_call(input.session.as_ref(), item.clone()).await {
            Ok(ResponseItemToolCall::ToolCall(call)) => {
                Ok(CompletedToolCallConversion::ToolCall(call))
            }
            Ok(ResponseItemToolCall::NotToolCall) => Ok(CompletedToolCallConversion::NotToolCall),
            Err(err) => {
                function_call_error_projection::project_function_call_error(input, item, err)
                    .await
                    .map(|()| CompletedToolCallConversion::FollowupRequired)
            }
        }
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod function_call_error_projection {
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::outcome::TurnError;
    use praxis_loop::outcome::TurnErrorKind;
    use praxis_protocol::models::ResponseItem;

    use crate::function_tool::FunctionCallError;

    use super::PraxisModelStreamInput;
    use super::tool_error_response::record_tool_error_response;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn project_function_call_error(
        input: &PraxisModelStreamInput,
        source_item: &ResponseItem,
        err: FunctionCallError,
    ) -> LoopResult<()> {
        match err {
            FunctionCallError::MissingLocalShellCallId => {
                record_missing_local_shell_call_id(input, source_item).await;
                Ok(())
            }
            FunctionCallError::RespondToModel(message) => {
                record_tool_error_response(input, source_item, message).await;
                Ok(())
            }
            FunctionCallError::Fatal(message) => Err(TurnError::new(TurnErrorKind::Tool, message)),
        }
    }

    async fn record_missing_local_shell_call_id(
        input: &PraxisModelStreamInput,
        source_item: &ResponseItem,
    ) {
        const MESSAGE: &str = "LocalShellCall without call_id or id";
        input
            .turn_context
            .session_telemetry
            .log_tool_failed("local_shell", MESSAGE);
        tracing::error!("{MESSAGE}");
        record_tool_error_response(input, source_item, MESSAGE).await;
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod token_usage_bridge {
    use praxis_loop::model::TokenUsage as LoopTokenUsage;
    use praxis_protocol::protocol::TokenUsage as ProtocolTokenUsage;

    pub(in crate::praxis::turn_loop_adapter::model_stream) fn protocol_to_loop(
        token_usage: Option<&ProtocolTokenUsage>,
    ) -> LoopTokenUsage {
        let Some(token_usage) = token_usage else {
            return LoopTokenUsage::default();
        };
        LoopTokenUsage {
            input: positive_u64(token_usage.input_tokens),
            output: positive_u64(token_usage.output_tokens),
            total: positive_u64(token_usage.total_tokens),
            reasoning: positive_u64(token_usage.reasoning_output_tokens),
        }
    }

    fn positive_u64(value: i64) -> u64 {
        value.max(0) as u64
    }
}

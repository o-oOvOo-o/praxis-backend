//! Turn hooks, continuation, compaction, steering, and stop decisions.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter) mod hooks {
    use std::sync::Arc;

    use async_trait::async_trait;
    use praxis_protocol::user_input::UserInput;
    use tokio_util::sync::CancellationToken;

    use super::super::Session;
    use super::super::TurnContext;
    use super::state::PraxisTurnBridgeState;
    use praxis_loop::decisions::ContextPressureDecision;
    use praxis_loop::decisions::ContextPressureView;
    use praxis_loop::decisions::PrepareContextDecision;
    use praxis_loop::decisions::PrepareContextView;
    use praxis_loop::decisions::RoundDecision;
    use praxis_loop::decisions::RoundOutcomeView;
    use praxis_loop::decisions::TurnStartDecision;
    use praxis_loop::decisions::TurnStopDecision;
    use praxis_loop::decisions::TurnStopView;
    use praxis_loop::hooks::TurnHooks;

    mod context_pressure {
        use praxis_loop::decisions::ContextPressureDecision;

        use super::super::compaction_decision;
        use super::PraxisTurnHooks;

        pub(in crate::praxis::turn_loop_adapter) async fn on_context_pressure(
            hooks: &PraxisTurnHooks,
        ) -> ContextPressureDecision {
            compaction_decision::context_pressure_decision(&hooks.session, &hooks.turn_context)
                .await
        }
    }
    mod followup {
        use praxis_loop::decisions::RoundDecision;

        use super::PraxisTurnHooks;

        mod compaction {
            use praxis_loop::TurnError;

            use super::super::super::compaction_decision;
            use super::super::super::compaction_refresh::PromptRefreshDecision;
            use super::super::PraxisTurnHooks;

            #[derive(Clone, Copy)]
            pub(in crate::praxis::turn_loop_adapter) enum FollowupCompaction {
                AfterToolRound,
                AfterFinalAnswerPendingInput,
            }

            pub(in crate::praxis::turn_loop_adapter) async fn refresh_followup_prompt(
                hooks: &PraxisTurnHooks,
                compaction: FollowupCompaction,
            ) -> Result<PromptRefreshDecision, TurnError> {
                match compaction {
                    FollowupCompaction::AfterToolRound => {
                        compaction_decision::compact_after_tool_round_if_needed(
                            &hooks.session,
                            &hooks.turn_context,
                        )
                        .await
                    }
                    FollowupCompaction::AfterFinalAnswerPendingInput => {
                        compaction_decision::compact_before_followup_after_model_round_if_needed(
                            &hooks.session,
                            &hooks.turn_context,
                        )
                        .await
                    }
                }
            }
        }
        mod intervention {
            use praxis_loop::TurnError;
            use praxis_protocol::models::DeveloperInstructions;
            use praxis_protocol::models::ResponseItem;

            use super::super::PraxisTurnHooks;

            pub(in crate::praxis::turn_loop_adapter) async fn record_pending_followup_intervention(
                hooks: &PraxisTurnHooks,
            ) -> Result<(), TurnError> {
                if let Some(message) = hooks
                    .turn_context
                    .tool_loop_guard
                    .take_followup_intervention()
                {
                    let intervention: ResponseItem = DeveloperInstructions::new(message).into();
                    hooks
                        .session
                        .record_conversation_items(
                            &hooks.turn_context,
                            std::slice::from_ref(&intervention),
                        )
                        .await;
                }
                Ok(())
            }
        }

        pub(in crate::praxis::turn_loop_adapter) use compaction::FollowupCompaction;

        pub(in crate::praxis::turn_loop_adapter) async fn continue_followup_round(
            hooks: &PraxisTurnHooks,
            compaction: FollowupCompaction,
        ) -> RoundDecision {
            if let Err(err) = intervention::record_pending_followup_intervention(hooks).await {
                return RoundDecision::Abort(err);
            }

            match compaction::refresh_followup_prompt(hooks, compaction).await {
                Ok(refresh) => RoundDecision::Continue {
                    prompt_update: refresh.into_round_prompt_update(),
                },
                Err(err) => RoundDecision::Abort(err),
            }
        }
    }
    mod prepare {
        use praxis_loop::decisions::PrepareContextDecision;
        use praxis_loop::outcome::TurnCompletionMessage;

        use super::super::prepare_phase::prepare_turn_before_model_request;
        use super::super::prompt_bridge;
        use super::PraxisTurnHooks;

        pub(in crate::praxis::turn_loop_adapter) async fn prepare_context(
            hooks: &PraxisTurnHooks,
        ) -> PrepareContextDecision {
            match prepare_turn_before_model_request(
                &hooks.session,
                &hooks.turn_context,
                &hooks.input,
                &hooks.cancellation_token,
            )
            .await
            {
                Some(outcome) => {
                    let prepared_items =
                        prompt_bridge::prompt_items_from_response_items(&outcome.prepared_items);
                    hooks.bridge_state.apply_prepare_outcome(outcome).await;
                    PrepareContextDecision::Prepared(prepared_items)
                }
                None => PrepareContextDecision::Stop(TurnCompletionMessage::NoMessage),
            }
        }
    }
    mod round {
        use praxis_loop::decisions::RoundDecision;
        use praxis_loop::decisions::RoundOutcomeView;
        use praxis_loop::outcome::RoundOutcome;
        use praxis_loop::outcome::TurnCompletionMessage;

        use super::PraxisTurnHooks;
        use super::followup;
        use super::followup::FollowupCompaction;

        pub(in crate::praxis::turn_loop_adapter) async fn after_model_round(
            hooks: &PraxisTurnHooks,
            view: RoundOutcomeView<'_>,
        ) -> RoundDecision {
            match view.outcome {
                RoundOutcome::FollowupRequired | RoundOutcome::ToolCalls { .. } => {
                    followup::continue_followup_round(hooks, FollowupCompaction::AfterToolRound)
                        .await
                }
                RoundOutcome::FinalAnswer { message } => {
                    if hooks
                        .session
                        .has_pending_input_bounded("model_round_completed")
                        .await
                    {
                        return followup::continue_followup_round(
                            hooks,
                            FollowupCompaction::AfterFinalAnswerPendingInput,
                        )
                        .await;
                    }
                    hooks.bridge_state.record_completion_message(message).await;
                    RoundDecision::Stop(message.clone())
                }
                RoundOutcome::TerminatedByTool { message } => {
                    hooks.bridge_state.record_completion_message(message).await;
                    RoundDecision::Stop(message.clone())
                }
                RoundOutcome::Empty => RoundDecision::Stop(TurnCompletionMessage::NoMessage),
            }
        }
    }
    mod start {
        use praxis_loop::TurnError;
        use praxis_loop::decisions::TurnStartDecision;

        use super::super::compaction_decision;
        use super::PraxisTurnHooks;

        pub(in crate::praxis::turn_loop_adapter) async fn on_turn_start(
            hooks: &PraxisTurnHooks,
        ) -> TurnStartDecision {
            if hooks.cancellation_token.is_cancelled() {
                return TurnStartDecision::Abort(TurnError::cancelled());
            }

            compaction_decision::before_model_request_compaction_decision(
                &hooks.session,
                &hooks.turn_context,
            )
            .await
        }
    }
    mod stop {
        use praxis_loop::TurnError;
        use praxis_loop::TurnErrorKind;
        use praxis_loop::decisions::TurnStopDecision;
        use praxis_loop::decisions::TurnStopView;

        use super::super::stop_hook_decision;
        use super::super::stop_hooks::TurnStopHooksDecision;
        use super::PraxisTurnHooks;

        pub(in crate::praxis::turn_loop_adapter) async fn on_turn_stop(
            hooks: &PraxisTurnHooks,
            view: TurnStopView<'_>,
        ) -> TurnStopDecision {
            let last_agent_message = match view.last_agent_message {
                Some(message) => Some(message.to_string()),
                None => hooks.bridge_state.last_agent_message().await,
            };
            let model_request_input_messages =
                hooks.bridge_state.model_request_input_messages().await;
            let decision = stop_hook_decision::run_stop_hooks(
                &hooks.session,
                &hooks.turn_context,
                &hooks.bridge_state,
                model_request_input_messages,
                last_agent_message,
            )
            .await;

            match decision {
                TurnStopHooksDecision::ContinueTurn => TurnStopDecision::ContinueTurn,
                TurnStopHooksDecision::CompleteTurn => TurnStopDecision::Complete,
                TurnStopHooksDecision::AbortTurn => TurnStopDecision::Abort(turn_error(
                    TurnErrorKind::Hook,
                    "turn completion hook aborted the turn",
                )),
            }
        }

        fn turn_error(kind: TurnErrorKind, err: impl std::fmt::Display) -> TurnError {
            TurnError::new(kind, err.to_string())
        }
    }

    pub(in crate::praxis::turn_loop_adapter) struct PraxisTurnHooks {
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
        bridge_state: Arc<PraxisTurnBridgeState>,
        cancellation_token: CancellationToken,
    }

    impl PraxisTurnHooks {
        pub(in crate::praxis::turn_loop_adapter) fn new(
            sess: Arc<Session>,
            turn_context: Arc<TurnContext>,
            input: Vec<UserInput>,
            bridge_state: Arc<PraxisTurnBridgeState>,
            cancellation_token: CancellationToken,
        ) -> Self {
            Self {
                session: sess,
                turn_context,
                input,
                bridge_state,
                cancellation_token,
            }
        }
    }

    #[async_trait]
    impl TurnHooks for PraxisTurnHooks {
        async fn on_turn_start(&self, _ctx: &praxis_loop::TurnContext) -> TurnStartDecision {
            start::on_turn_start(self).await
        }

        async fn on_context_pressure(
            &self,
            _view: ContextPressureView<'_>,
        ) -> ContextPressureDecision {
            context_pressure::on_context_pressure(self).await
        }

        async fn prepare_context(&self, _view: PrepareContextView<'_>) -> PrepareContextDecision {
            prepare::prepare_context(self).await
        }

        async fn after_model_round(&self, view: RoundOutcomeView<'_>) -> RoundDecision {
            round::after_model_round(self, view).await
        }

        async fn on_turn_stop(&self, view: TurnStopView<'_>) -> TurnStopDecision {
            stop::on_turn_stop(self, view).await
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod stop_hooks {
    use std::sync::Arc;

    use super::super::Session;
    use super::super::TurnContext;

    mod after_agent {
        use std::sync::Arc;

        use praxis_hooks::HookEvent;
        use praxis_hooks::HookEventAfterAgent;
        use praxis_hooks::HookPayload;
        use praxis_hooks::HookResponse;
        use praxis_hooks::HookResult;
        use tracing::warn;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::TurnStopHooksDecision;

        pub(in crate::praxis::turn_loop_adapter) async fn run_after_agent_hooks(
            sess: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            model_request_input_messages: Vec<String>,
            last_agent_message: Option<String>,
        ) -> TurnStopHooksDecision {
            let hook_outcomes = sess
                .hooks()
                .dispatch(HookPayload {
                    session_id: sess.conversation_id,
                    cwd: turn_context.cwd.to_path_buf(),
                    client: turn_context.app_gateway_client_name.clone(),
                    triggered_at: chrono::Utc::now(),
                    hook_event: HookEvent::AfterAgent {
                        event: HookEventAfterAgent {
                            thread_id: sess.conversation_id,
                            turn_id: turn_context.sub_id.clone(),
                            input_messages: model_request_input_messages,
                            last_assistant_message: last_agent_message,
                        },
                    },
                })
                .await;

            if let Some(message) = first_abort_message(turn_context, hook_outcomes) {
                sess.turn_event_emitter(turn_context)
                    .error(message, None)
                    .await;
                TurnStopHooksDecision::AbortTurn
            } else {
                TurnStopHooksDecision::CompleteTurn
            }
        }

        fn first_abort_message(
            turn_context: &TurnContext,
            hook_outcomes: Vec<HookResponse>,
        ) -> Option<String> {
            let mut abort_message = None;
            for hook_outcome in hook_outcomes {
                let hook_name = hook_outcome.hook_name;
                match hook_outcome.result {
                    HookResult::Success => {}
                    HookResult::FailedContinue(error) => {
                        warn!(
                            turn_id = %turn_context.sub_id,
                            hook_name = %hook_name,
                            error = %error,
                            "after_agent hook failed; continuing"
                        );
                    }
                    HookResult::FailedAbort(error) => {
                        let message = format!(
                            "after_agent hook '{hook_name}' failed and aborted turn completion: {error}"
                        );
                        warn!(
                            turn_id = %turn_context.sub_id,
                            hook_name = %hook_name,
                            error = %error,
                            "after_agent hook failed; aborting operation"
                        );
                        if abort_message.is_none() {
                            abort_message = Some(message);
                        }
                    }
                }
            }
            abort_message
        }
    }
    mod stop_lifecycle {
        use std::sync::Arc;

        use praxis_protocol::items::build_hook_prompt_message;
        use praxis_protocol::protocol::AskForApproval;
        use praxis_protocol::protocol::EventMsg;

        use super::super::super::Session;
        use super::super::super::TurnContext;

        pub(in crate::praxis::turn_loop_adapter) enum StopHookLifecycleDecision {
            ContinueTurn,
            CompleteTurn,
            RunAfterAgent,
        }

        pub(in crate::praxis::turn_loop_adapter) async fn run_stop_hook_lifecycle(
            sess: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            last_agent_message: Option<String>,
            stop_hook_active: &mut bool,
        ) -> StopHookLifecycleDecision {
            let stop_request =
                build_stop_request(sess, turn_context, last_agent_message, *stop_hook_active).await;
            emit_stop_hook_starts(sess, turn_context, &stop_request).await;
            let stop_outcome = sess.hooks().run_stop(stop_request).await;

            for completed in stop_outcome.hook_events {
                sess.send_event(turn_context, EventMsg::HookCompleted(completed))
                    .await;
            }

            if stop_outcome.should_block {
                if let Some(hook_prompt_message) =
                    build_hook_prompt_message(&stop_outcome.continuation_fragments)
                {
                    sess.record_conversation_items(
                        turn_context,
                        std::slice::from_ref(&hook_prompt_message),
                    )
                    .await;
                    *stop_hook_active = true;
                    return StopHookLifecycleDecision::ContinueTurn;
                }
                sess.turn_event_emitter(turn_context)
                    .warning(
                        "Stop hook requested continuation without a prompt; ignoring the block.",
                    )
                    .await;
            }

            if stop_outcome.should_stop {
                StopHookLifecycleDecision::CompleteTurn
            } else {
                StopHookLifecycleDecision::RunAfterAgent
            }
        }

        async fn build_stop_request(
            sess: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            last_agent_message: Option<String>,
            stop_hook_active: bool,
        ) -> praxis_hooks::StopRequest {
            praxis_hooks::StopRequest {
                session_id: sess.conversation_id,
                turn_id: turn_context.sub_id.clone(),
                cwd: turn_context.cwd.to_path_buf(),
                transcript_path: sess.hook_transcript_path().await,
                model: turn_context.model_info.slug.clone(),
                permission_mode: stop_hook_permission_mode(turn_context),
                stop_hook_active,
                last_assistant_message: last_agent_message,
            }
        }

        async fn emit_stop_hook_starts(
            sess: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            stop_request: &praxis_hooks::StopRequest,
        ) {
            for run in sess.hooks().preview_stop(stop_request) {
                sess.send_event(
                    turn_context,
                    EventMsg::HookStarted(praxis_protocol::protocol::HookStartedEvent {
                        turn_id: Some(turn_context.sub_id.clone()),
                        run,
                    }),
                )
                .await;
            }
        }

        fn stop_hook_permission_mode(turn_context: &TurnContext) -> String {
            match turn_context.effective_approval_policy() {
                AskForApproval::Never => "bypassPermissions",
                AskForApproval::UnlessTrusted
                | AskForApproval::OnFailure
                | AskForApproval::OnRequest
                | AskForApproval::Granular(_) => "default",
            }
            .to_string()
        }
    }

    use after_agent::run_after_agent_hooks;
    use stop_lifecycle::StopHookLifecycleDecision;
    use stop_lifecycle::run_stop_hook_lifecycle;

    pub(in crate::praxis::turn_loop_adapter) enum TurnStopHooksDecision {
        ContinueTurn,
        CompleteTurn,
        AbortTurn,
    }

    pub(in crate::praxis::turn_loop_adapter) async fn run_turn_completion_hooks(
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        model_request_input_messages: Vec<String>,
        last_agent_message: Option<String>,
        stop_hook_active: &mut bool,
    ) -> TurnStopHooksDecision {
        match run_stop_hook_lifecycle(
            sess,
            turn_context,
            last_agent_message.clone(),
            stop_hook_active,
        )
        .await
        {
            StopHookLifecycleDecision::ContinueTurn => TurnStopHooksDecision::ContinueTurn,
            StopHookLifecycleDecision::CompleteTurn => TurnStopHooksDecision::CompleteTurn,
            StopHookLifecycleDecision::RunAfterAgent => {
                run_after_agent_hooks(
                    sess,
                    turn_context,
                    model_request_input_messages,
                    last_agent_message,
                )
                .await
            }
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod stop_hook_decision {
    use std::sync::Arc;

    use super::super::Session;
    use super::super::TurnContext;
    use super::state::PraxisTurnBridgeState;
    use super::stop_hooks::TurnStopHooksDecision;
    use super::stop_hooks::run_turn_completion_hooks;

    pub(in crate::praxis::turn_loop_adapter) async fn run_stop_hooks(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        bridge_state: &Arc<PraxisTurnBridgeState>,
        input_messages: Vec<String>,
        last_agent_message: Option<String>,
    ) -> TurnStopHooksDecision {
        bridge_state
            .record_optional_agent_message(last_agent_message.clone())
            .await;
        let mut stop_hook_active = bridge_state.stop_hook_active().await;
        let decision = run_turn_completion_hooks(
            session,
            turn_context,
            input_messages,
            last_agent_message,
            &mut stop_hook_active,
        )
        .await;
        bridge_state.set_stop_hook_active(stop_hook_active).await;
        decision
    }
}

pub(in crate::praxis::turn_loop_adapter) mod steering_decision {
    use std::sync::Arc;

    use praxis_loop::model::SteeringMessage;
    use praxis_loop::outcome::TurnCompletionMessage;
    use praxis_loop::services::SteeringControl;
    use praxis_loop::services::SteeringDrain;

    use crate::hook_runtime::process_pending_input_for_model_round;
    use crate::hook_runtime::run_pending_session_start_hooks;

    use super::super::Session;
    use super::super::TurnContext;
    use super::prompt_bridge;

    pub(in crate::praxis::turn_loop_adapter) async fn process_pending_input_for_round(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
    ) -> SteeringDrain {
        if run_pending_session_start_hooks(session, turn_context).await {
            return SteeringDrain {
                messages: Vec::new(),
                control: SteeringControl::StopWithoutModelRequest(TurnCompletionMessage::NoMessage),
            };
        }

        let pending_input = session.get_pending_inputs().await;
        let outcome =
            process_pending_input_for_model_round(session, turn_context, pending_input).await;
        let prompt_items =
            prompt_bridge::prompt_items_from_response_items(outcome.accepted_response_items());
        let control = if outcome.should_retry_without_model_request() {
            SteeringControl::RetryWithoutModelRequest
        } else if outcome.should_stop_without_model_request() {
            SteeringControl::StopWithoutModelRequest(TurnCompletionMessage::NoMessage)
        } else {
            SteeringControl::Continue
        };
        let messages = if matches!(&control, SteeringControl::Continue) && !prompt_items.is_empty()
        {
            vec![SteeringMessage::new(prompt_items)]
        } else {
            Vec::new()
        };

        SteeringDrain { messages, control }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod compaction_decision {
    mod before_model_request {
        use std::sync::Arc;

        use praxis_loop::decisions::TurnStartDecision;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::super::turn_compaction::run_before_model_request_compact;
        use super::super::compaction_refresh;

        pub(in crate::praxis::turn_loop_adapter) async fn before_model_request_compaction_decision(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
        ) -> TurnStartDecision {
            match run_before_model_request_compact(session, turn_context).await {
                Ok(false) => TurnStartDecision::Proceed,
                Ok(true) => TurnStartDecision::ReplaceInitialPrompt(
                    compaction_refresh::prompt_items_from_session_history(session, turn_context)
                        .await,
                ),
                Err(err) => {
                    let error_event = err.to_error_event(/*message_prefix*/ None);
                    turn_context
                        .tool_loop_guard
                        .record_terminal_model_error(error_event.message.clone());
                    TurnStartDecision::Abort(compaction_refresh::internal_turn_error(
                        error_event.message,
                    ))
                }
            }
        }
    }
    mod context_pressure {
        use std::sync::Arc;

        use praxis_loop::decisions::ContextPressureDecision;

        use crate::compact::InitialContextInjection;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::compaction_refresh;

        pub(in crate::praxis::turn_loop_adapter) async fn context_pressure_decision(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
        ) -> ContextPressureDecision {
            if !compaction_refresh::auto_compact_needed(session, turn_context).await {
                return ContextPressureDecision::Proceed;
            }

            match compaction_refresh::auto_compact_prompt_refresh(
                session,
                turn_context,
                InitialContextInjection::DoNotInject,
            )
            .await
            {
                Ok(prompt_items) => ContextPressureDecision::Compacted {
                    prompt_items,
                    transcript_items: Vec::new(),
                },
                Err(err) => ContextPressureDecision::Abort(err),
            }
        }
    }
    mod followup {
        use std::sync::Arc;

        use praxis_loop::TurnError;

        use crate::compact::InitialContextInjection;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::compaction_refresh;

        type PromptRefreshDecision = compaction_refresh::PromptRefreshDecision;

        pub(in crate::praxis::turn_loop_adapter) async fn compact_after_tool_round_if_needed(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
        ) -> Result<PromptRefreshDecision, TurnError> {
            refresh_before_last_user_message_if_needed(session, turn_context).await
        }

        pub(in crate::praxis::turn_loop_adapter) async fn compact_before_followup_after_model_round_if_needed(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
        ) -> Result<PromptRefreshDecision, TurnError> {
            refresh_before_last_user_message_if_needed(session, turn_context).await
        }

        async fn refresh_before_last_user_message_if_needed(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
        ) -> Result<PromptRefreshDecision, TurnError> {
            compaction_refresh::auto_compact_prompt_refresh_if_needed(
                session,
                turn_context,
                InitialContextInjection::BeforeLastUserMessage,
            )
            .await
        }
    }

    pub(in crate::praxis::turn_loop_adapter) use before_model_request::before_model_request_compaction_decision;
    pub(in crate::praxis::turn_loop_adapter) use context_pressure::context_pressure_decision;
    pub(in crate::praxis::turn_loop_adapter) use followup::compact_after_tool_round_if_needed;
    pub(in crate::praxis::turn_loop_adapter) use followup::compact_before_followup_after_model_round_if_needed;
}

pub(in crate::praxis::turn_loop_adapter) mod compaction_refresh {
    mod error {
        use praxis_loop::TurnError;
        use praxis_loop::TurnErrorKind;

        pub(in crate::praxis::turn_loop_adapter) fn internal_turn_error(
            err: impl std::fmt::Display,
        ) -> TurnError {
            TurnError::new(TurnErrorKind::Internal, err.to_string())
        }
    }
    mod policy {
        use std::sync::Arc;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::super::turn_compaction::effective_auto_compact_token_limit;

        pub(in crate::praxis::turn_loop_adapter) async fn auto_compact_needed(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
        ) -> bool {
            let total_usage_tokens = session.get_total_token_usage().await;
            total_usage_tokens >= auto_compact_limit_or_max(session, turn_context).await
        }

        async fn auto_compact_limit_or_max(session: &Session, turn_context: &TurnContext) -> i64 {
            effective_auto_compact_token_limit(session, turn_context)
                .await
                .unwrap_or(i64::MAX)
        }
    }
    mod prompt_refresh {
        use std::sync::Arc;

        use praxis_loop::TurnError;

        use crate::compact::InitialContextInjection;

        use super::super::super::Session;
        use super::super::super::TurnContext;
        use super::super::super::turn_compaction::run_auto_compact;
        use super::super::prompt_bridge;
        use super::LoopPromptItems;
        use super::PromptRefreshDecision;
        use super::error;
        use super::policy;

        pub(in crate::praxis::turn_loop_adapter) async fn prompt_items_from_session_history(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
        ) -> LoopPromptItems {
            prompt_bridge::initial_prompt_items_from_session_history(session, turn_context).await
        }

        pub(in crate::praxis::turn_loop_adapter) async fn auto_compact_prompt_refresh(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            injection: InitialContextInjection,
        ) -> Result<LoopPromptItems, TurnError> {
            run_auto_compact(session, turn_context, injection)
                .await
                .map_err(error::internal_turn_error)?;
            Ok(prompt_items_from_session_history(session, turn_context).await)
        }

        pub(in crate::praxis::turn_loop_adapter) async fn auto_compact_prompt_refresh_if_needed(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            injection: InitialContextInjection,
        ) -> Result<PromptRefreshDecision, TurnError> {
            if !policy::auto_compact_needed(session, turn_context).await {
                return Ok(PromptRefreshDecision::Unchanged);
            }
            auto_compact_prompt_refresh(session, turn_context, injection)
                .await
                .map(PromptRefreshDecision::Refreshed)
        }
    }
    use praxis_loop::decisions::RoundPromptUpdate;

    pub(in crate::praxis::turn_loop_adapter) use error::internal_turn_error;
    pub(in crate::praxis::turn_loop_adapter) use policy::auto_compact_needed;
    pub(in crate::praxis::turn_loop_adapter) use prompt_refresh::auto_compact_prompt_refresh;
    pub(in crate::praxis::turn_loop_adapter) use prompt_refresh::auto_compact_prompt_refresh_if_needed;
    pub(in crate::praxis::turn_loop_adapter) use prompt_refresh::prompt_items_from_session_history;

    pub(in crate::praxis::turn_loop_adapter) type LoopPromptItems =
        Vec<praxis_loop::model::PromptItem>;

    pub(in crate::praxis::turn_loop_adapter) enum PromptRefreshDecision {
        Unchanged,
        Refreshed(LoopPromptItems),
    }

    impl PromptRefreshDecision {
        pub(in crate::praxis::turn_loop_adapter) fn into_round_prompt_update(
            self,
        ) -> RoundPromptUpdate {
            match self {
                Self::Unchanged => RoundPromptUpdate::Reuse,
                Self::Refreshed(prompt_items) => RoundPromptUpdate::Replace(prompt_items),
            }
        }
    }
}

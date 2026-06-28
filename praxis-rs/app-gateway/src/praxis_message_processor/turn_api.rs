use std::sync::Arc;

use praxis_app_gateway_protocol::ApprovalsReviewer;
use praxis_app_gateway_protocol::AskForApproval;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::PraxisErrorInfo as AppGatewayErrorInfo;
use praxis_app_gateway_protocol::ReviewDelivery as ApiReviewDelivery;
use praxis_app_gateway_protocol::ReviewStartParams;
use praxis_app_gateway_protocol::ReviewStartResponse;
use praxis_app_gateway_protocol::ReviewTarget as ApiReviewTarget;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadRealtimeAppendAudioParams;
use praxis_app_gateway_protocol::ThreadRealtimeAppendAudioResponse;
use praxis_app_gateway_protocol::ThreadRealtimeAppendTextParams;
use praxis_app_gateway_protocol::ThreadRealtimeAppendTextResponse;
use praxis_app_gateway_protocol::ThreadRealtimeStartParams;
use praxis_app_gateway_protocol::ThreadRealtimeStartResponse;
use praxis_app_gateway_protocol::ThreadRealtimeStopParams;
use praxis_app_gateway_protocol::ThreadRealtimeStopResponse;
use praxis_app_gateway_protocol::ThreadStartedNotification;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnError;
use praxis_app_gateway_protocol::TurnInterruptParams;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_app_gateway_protocol::TurnSteerParams;
use praxis_app_gateway_protocol::TurnSteerResponse;
use praxis_app_gateway_protocol::UserInput as ApiUserInput;
use praxis_core::PraxisThread;
use praxis_core::SteerInputError;
use praxis_core::ThreadForkSnapshot;
use praxis_core::ThreadSpawnResult;
use praxis_core::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use praxis_features::Feature;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::protocol::ConversationAudioParams;
use praxis_protocol::protocol::ConversationStartParams;
use praxis_protocol::protocol::ConversationTextParams;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::ReviewDelivery as CoreReviewDelivery;
use praxis_protocol::protocol::ReviewRequest;
use praxis_protocol::protocol::ReviewTarget as CoreReviewTarget;
use praxis_protocol::user_input::MAX_USER_INPUT_TEXT_CHARS;
use praxis_protocol::user_input::UserInput as CoreInputItem;

use super::EnsureConversationListenerResult;
use super::PraxisMessageProcessor;
use super::find_thread_rollout_path_or_no_rollout;
use super::load_thread_summary_for_rollout;
use crate::error_code::INPUT_TOO_LARGE_ERROR_CODE;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;

impl PraxisMessageProcessor {
    /// If a client sends `developer_instructions: null` during a mode switch,
    /// use the built-in instructions for that mode.
    fn normalize_turn_start_collaboration_mode(
        &self,
        mut collaboration_mode: CollaborationMode,
        collaboration_modes_config: CollaborationModesConfig,
    ) -> CollaborationMode {
        if collaboration_mode.settings.developer_instructions.is_none()
            && let Some(instructions) = self
                .thread_manager
                .get_models_manager()
                .list_collaboration_modes_for_config(collaboration_modes_config)
                .into_iter()
                .find(|preset| preset.mode == Some(collaboration_mode.mode))
                .and_then(|preset| preset.developer_instructions.flatten())
                .filter(|instructions| !instructions.is_empty())
        {
            collaboration_mode.settings.developer_instructions = Some(instructions);
        }

        collaboration_mode
    }

    fn review_request_from_target(
        target: ApiReviewTarget,
    ) -> Result<(ReviewRequest, String), JSONRPCErrorError> {
        fn invalid_request(message: String) -> JSONRPCErrorError {
            JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message,
                data: None,
            }
        }

        let cleaned_target = match target {
            ApiReviewTarget::UncommittedChanges => ApiReviewTarget::UncommittedChanges,
            ApiReviewTarget::BaseBranch { branch } => {
                let branch = branch.trim().to_string();
                if branch.is_empty() {
                    return Err(invalid_request("branch must not be empty".to_string()));
                }
                ApiReviewTarget::BaseBranch { branch }
            }
            ApiReviewTarget::Commit { sha, title } => {
                let sha = sha.trim().to_string();
                if sha.is_empty() {
                    return Err(invalid_request("sha must not be empty".to_string()));
                }
                let title = title
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty());
                ApiReviewTarget::Commit { sha, title }
            }
            ApiReviewTarget::Custom { instructions } => {
                let trimmed = instructions.trim().to_string();
                if trimmed.is_empty() {
                    return Err(invalid_request(
                        "instructions must not be empty".to_string(),
                    ));
                }
                ApiReviewTarget::Custom {
                    instructions: trimmed,
                }
            }
        };

        let core_target = match cleaned_target {
            ApiReviewTarget::UncommittedChanges => CoreReviewTarget::UncommittedChanges,
            ApiReviewTarget::BaseBranch { branch } => CoreReviewTarget::BaseBranch { branch },
            ApiReviewTarget::Commit { sha, title } => CoreReviewTarget::Commit { sha, title },
            ApiReviewTarget::Custom { instructions } => CoreReviewTarget::Custom { instructions },
        };

        let hint = praxis_core::review_prompts::user_facing_hint(&core_target);
        let review_request = ReviewRequest {
            target: core_target,
            user_facing_hint: Some(hint.clone()),
        };

        Ok((review_request, hint))
    }

    fn input_too_large_error(actual_chars: usize) -> JSONRPCErrorError {
        JSONRPCErrorError {
            code: INVALID_PARAMS_ERROR_CODE,
            message: format!(
                "Input exceeds the maximum length of {MAX_USER_INPUT_TEXT_CHARS} characters."
            ),
            data: Some(serde_json::json!({
                "input_error_code": INPUT_TOO_LARGE_ERROR_CODE,
                "max_chars": MAX_USER_INPUT_TEXT_CHARS,
                "actual_chars": actual_chars,
            })),
        }
    }

    fn validate_api_input_limit(items: &[ApiUserInput]) -> Result<(), JSONRPCErrorError> {
        let actual_chars: usize = items.iter().map(ApiUserInput::text_char_count).sum();
        if actual_chars > MAX_USER_INPUT_TEXT_CHARS {
            return Err(Self::input_too_large_error(actual_chars));
        }
        Ok(())
    }

    pub(super) async fn turn_start(
        &self,
        request_id: ConnectionRequestId,
        params: TurnStartParams,
        app_gateway_client_name: Option<String>,
    ) {
        if let Err(error) = Self::validate_api_input_limit(&params.input) {
            self.outgoing.send_error(request_id, error).await;
            return;
        }
        let TurnStartParams {
            thread_id,
            input,
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox_policy,
            model_provider,
            model,
            service_tier,
            effort,
            summary,
            personality,
            output_schema,
            collaboration_mode,
        } = params;
        let Some((_, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };
        if let Err(error) =
            Self::set_app_gateway_client_name(thread.as_ref(), app_gateway_client_name).await
        {
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let collaboration_modes_config = CollaborationModesConfig {
            default_mode_request_user_input: thread.enabled(Feature::DefaultModeRequestUserInput),
        };
        let collaboration_mode = collaboration_mode.map(|mode| {
            self.normalize_turn_start_collaboration_mode(mode, collaboration_modes_config)
        });

        let mapped_items: Vec<CoreInputItem> =
            input.into_iter().map(ApiUserInput::into_core).collect();
        let snapshot = thread.config_snapshot().await;
        let op = Op::UserTurn {
            items: mapped_items,
            cwd: cwd.unwrap_or(snapshot.cwd),
            approval_policy: approval_policy
                .map(AskForApproval::to_core)
                .unwrap_or(snapshot.approval_policy),
            approvals_reviewer: Some(
                approvals_reviewer
                    .map(ApprovalsReviewer::to_core)
                    .unwrap_or(snapshot.approvals_reviewer),
            ),
            sandbox_policy: sandbox_policy
                .map(|policy| policy.to_core())
                .unwrap_or(snapshot.sandbox_policy),
            model: model.unwrap_or(snapshot.model),
            model_provider: Some(model_provider.unwrap_or(snapshot.model_provider_id)),
            effort: effort.or(snapshot.reasoning_effort),
            summary,
            service_tier: service_tier.or(Some(snapshot.service_tier)),
            final_output_json_schema: output_schema,
            collaboration_mode,
            personality: personality.or(snapshot.personality),
        };

        // Start the turn by submitting the user input. Return its submission id as turn_id.
        let turn_id = self.submit_core_op(&request_id, thread.as_ref(), op).await;

        match turn_id {
            Ok(turn_id) => {
                self.outgoing
                    .record_request_turn_id(&request_id, &turn_id)
                    .await;
                let turn = Turn {
                    id: turn_id.clone(),
                    items: vec![],
                    error: None,
                    status: TurnStatus::InProgress,
                };

                let response = TurnStartResponse { turn };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to start turn: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn set_app_gateway_client_name(
        thread: &PraxisThread,
        app_gateway_client_name: Option<String>,
    ) -> Result<(), JSONRPCErrorError> {
        thread
            .set_app_gateway_client_name(app_gateway_client_name)
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to set app gateway client name: {err}"),
                data: None,
            })
    }

    pub(super) async fn turn_steer(
        &self,
        request_id: ConnectionRequestId,
        params: TurnSteerParams,
    ) {
        let Some((_, thread)) = self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };

        if params.expected_turn_id.is_empty() {
            self.send_invalid_request_error(
                request_id,
                "expectedTurnId must not be empty".to_string(),
            )
            .await;
            return;
        }
        self.outgoing
            .record_request_turn_id(&request_id, &params.expected_turn_id)
            .await;
        if let Err(error) = Self::validate_api_input_limit(&params.input) {
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let mapped_items: Vec<CoreInputItem> = params
            .input
            .into_iter()
            .map(ApiUserInput::into_core)
            .collect();

        match thread
            .steer_input(mapped_items, Some(&params.expected_turn_id))
            .await
        {
            Ok(turn_id) => {
                let response = TurnSteerResponse { turn_id };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                let (code, message, data) = match err {
                    SteerInputError::NoActiveTurn(_) => (
                        INVALID_REQUEST_ERROR_CODE,
                        "no active turn to steer".to_string(),
                        None,
                    ),
                    SteerInputError::ExpectedTurnMismatch { expected, actual } => (
                        INVALID_REQUEST_ERROR_CODE,
                        format!("expected active turn id `{expected}` but found `{actual}`"),
                        None,
                    ),
                    SteerInputError::ActiveTurnNotSteerable { turn_kind } => {
                        let message = match turn_kind {
                            praxis_protocol::protocol::NonSteerableTurnKind::Review => {
                                "cannot steer a review turn".to_string()
                            }
                            praxis_protocol::protocol::NonSteerableTurnKind::Compact => {
                                "cannot steer a compact turn".to_string()
                            }
                        };
                        let error = TurnError {
                            message: message.clone(),
                            praxis_error_info: Some(AppGatewayErrorInfo::ActiveTurnNotSteerable {
                                turn_kind: turn_kind.into(),
                            }),
                            additional_details: None,
                        };
                        let data = match serde_json::to_value(error) {
                            Ok(data) => Some(data),
                            Err(error) => {
                                tracing::error!(
                                    ?error,
                                    "failed to serialize active-turn-not-steerable turn error"
                                );
                                None
                            }
                        };
                        (INVALID_REQUEST_ERROR_CODE, message, data)
                    }
                    SteerInputError::EmptyInput => (
                        INVALID_REQUEST_ERROR_CODE,
                        "input must not be empty".to_string(),
                        None,
                    ),
                };
                let error = JSONRPCErrorError {
                    code,
                    message,
                    data,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn prepare_realtime_conversation_thread(
        &mut self,
        request_id: ConnectionRequestId,
        thread_id: &str,
    ) -> Option<(ThreadId, Arc<PraxisThread>)> {
        let (thread_id, thread) = self
            .ensure_thread_for_request(thread_id, &request_id)
            .await?;

        match self
            .ensure_conversation_listener(
                thread_id,
                request_id.connection_id,
                /*raw_events_enabled*/ false,
            )
            .await
        {
            Ok(EnsureConversationListenerResult::Attached) => {}
            Ok(EnsureConversationListenerResult::ConnectionClosed) => {
                return None;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return None;
            }
        }

        if !thread.enabled(Feature::RealtimeConversation) {
            self.send_invalid_request_error(
                request_id,
                format!("thread {thread_id} does not support realtime conversation"),
            )
            .await;
            return None;
        }

        Some((thread_id, thread))
    }

    pub(super) async fn thread_realtime_start(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadRealtimeStartParams,
    ) {
        let Some((_, thread)) = self
            .prepare_realtime_conversation_thread(request_id.clone(), &params.thread_id)
            .await
        else {
            return;
        };

        let submit = self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                Op::RealtimeConversationStart(ConversationStartParams {
                    prompt: params.prompt,
                    session_id: params.session_id,
                }),
            )
            .await;

        match submit {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadRealtimeStartResponse::default())
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to start realtime conversation: {err}"),
                )
                .await;
            }
        }
    }

    pub(super) async fn thread_realtime_append_audio(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadRealtimeAppendAudioParams,
    ) {
        let Some((_, thread)) = self
            .prepare_realtime_conversation_thread(request_id.clone(), &params.thread_id)
            .await
        else {
            return;
        };

        let submit = self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                Op::RealtimeConversationAudio(ConversationAudioParams {
                    frame: params.audio.into(),
                }),
            )
            .await;

        match submit {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadRealtimeAppendAudioResponse::default())
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to append realtime conversation audio: {err}"),
                )
                .await;
            }
        }
    }

    pub(super) async fn thread_realtime_append_text(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadRealtimeAppendTextParams,
    ) {
        let Some((_, thread)) = self
            .prepare_realtime_conversation_thread(request_id.clone(), &params.thread_id)
            .await
        else {
            return;
        };

        let submit = self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                Op::RealtimeConversationText(ConversationTextParams { text: params.text }),
            )
            .await;

        match submit {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadRealtimeAppendTextResponse::default())
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to append realtime conversation text: {err}"),
                )
                .await;
            }
        }
    }

    pub(super) async fn thread_realtime_stop(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadRealtimeStopParams,
    ) {
        let Some((_, thread)) = self
            .prepare_realtime_conversation_thread(request_id.clone(), &params.thread_id)
            .await
        else {
            return;
        };

        let submit = self
            .submit_core_op(&request_id, thread.as_ref(), Op::RealtimeConversationClose)
            .await;

        match submit {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadRealtimeStopResponse::default())
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to stop realtime conversation: {err}"),
                )
                .await;
            }
        }
    }

    fn build_review_turn(turn_id: String, display_text: &str) -> Turn {
        let items = if display_text.is_empty() {
            Vec::new()
        } else {
            vec![ThreadItem::UserMessage {
                id: turn_id.clone(),
                content: vec![ApiUserInput::Text {
                    text: display_text.to_string(),
                    // Review prompt display text is synthesized; no UI element ranges to preserve.
                    text_elements: Vec::new(),
                }],
            }]
        };

        Turn {
            id: turn_id,
            items,
            error: None,
            status: TurnStatus::InProgress,
        }
    }

    async fn emit_review_started(
        &self,
        request_id: &ConnectionRequestId,
        turn: Turn,
        review_thread_id: String,
    ) {
        let response = ReviewStartResponse {
            turn,
            review_thread_id,
        };
        self.outgoing
            .send_response(request_id.clone(), response)
            .await;
    }

    async fn start_inline_review(
        &self,
        request_id: &ConnectionRequestId,
        parent_thread: Arc<PraxisThread>,
        review_request: ReviewRequest,
        display_text: &str,
        parent_thread_id: String,
    ) -> std::result::Result<(), JSONRPCErrorError> {
        let turn_id = self
            .submit_core_op(
                request_id,
                parent_thread.as_ref(),
                Op::Review { review_request },
            )
            .await;

        match turn_id {
            Ok(turn_id) => {
                let turn = Self::build_review_turn(turn_id, display_text);
                self.emit_review_started(request_id, turn, parent_thread_id)
                    .await;
                Ok(())
            }
            Err(err) => Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to start review: {err}"),
                data: None,
            }),
        }
    }

    async fn start_detached_review(
        &mut self,
        request_id: &ConnectionRequestId,
        parent_thread_id: ThreadId,
        parent_thread: Arc<PraxisThread>,
        review_request: ReviewRequest,
        display_text: &str,
    ) -> std::result::Result<(), JSONRPCErrorError> {
        let rollout_path = if let Some(path) = parent_thread.rollout_path() {
            path
        } else {
            find_thread_rollout_path_or_no_rollout(&self.config, parent_thread_id).await?
        };

        let mut config = self.config.as_ref().clone();
        if let Some(review_model) = &config.review_model {
            config.model = Some(review_model.clone());
        }

        let ThreadSpawnResult {
            thread_id,
            thread: review_thread,
            session_configured,
            ..
        } = self
            .thread_manager
            .fork_thread(
                ThreadForkSnapshot::Interrupted,
                config,
                rollout_path,
                /*persist_extended_history*/ false,
                self.request_trace_context(request_id).await,
            )
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("error creating detached review thread: {err}"),
                data: None,
            })?;

        Self::log_listener_attach_result(
            self.ensure_conversation_listener(
                thread_id,
                request_id.connection_id,
                /*raw_events_enabled*/ false,
            )
            .await,
            thread_id,
            request_id.connection_id,
            "review thread",
        );

        let fallback_provider = self.config.model_provider_id.as_str();
        if let Some(rollout_path) = review_thread.rollout_path() {
            match load_thread_summary_for_rollout(
                &self.config,
                thread_id,
                rollout_path.as_path(),
                fallback_provider,
                None,
            )
            .await
            {
                Ok(mut thread) => {
                    self.thread_watch_manager
                        .upsert_thread_silently(thread.clone())
                        .await;
                    self.project_thread_runtime_state(
                        &mut thread,
                        /*has_live_in_progress_turn*/ false,
                    )
                    .await;
                    let notif = ThreadStartedNotification { thread };
                    self.outgoing
                        .send_server_notification(ServerNotification::ThreadStarted(notif))
                        .await;
                }
                Err(err) => {
                    tracing::warn!(
                        "failed to load summary for review thread {}: {}",
                        session_configured.session_id,
                        err
                    );
                }
            }
        } else {
            tracing::warn!(
                "review thread {} has no rollout path",
                session_configured.session_id
            );
        }

        let turn_id = self
            .submit_core_op(
                request_id,
                review_thread.as_ref(),
                Op::Review { review_request },
            )
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to start detached review turn: {err}"),
                data: None,
            })?;

        let turn = Self::build_review_turn(turn_id, display_text);
        let review_thread_id = thread_id.to_string();
        self.emit_review_started(request_id, turn, review_thread_id)
            .await;

        Ok(())
    }

    pub(super) async fn review_start(
        &mut self,
        request_id: ConnectionRequestId,
        params: ReviewStartParams,
    ) {
        let ReviewStartParams {
            thread_id,
            target,
            delivery,
        } = params;
        let Some((parent_thread_id, parent_thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        let (review_request, display_text) = match Self::review_request_from_target(target) {
            Ok(value) => value,
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
                return;
            }
        };

        let delivery = delivery.unwrap_or(ApiReviewDelivery::Inline).to_core();
        match delivery {
            CoreReviewDelivery::Inline => {
                if let Err(err) = self
                    .start_inline_review(
                        &request_id,
                        parent_thread,
                        review_request,
                        display_text.as_str(),
                        thread_id.clone(),
                    )
                    .await
                {
                    self.outgoing.send_error(request_id, err).await;
                }
            }
            CoreReviewDelivery::Detached => {
                if let Err(err) = self
                    .start_detached_review(
                        &request_id,
                        parent_thread_id,
                        parent_thread,
                        review_request,
                        display_text.as_str(),
                    )
                    .await
                {
                    self.outgoing.send_error(request_id, err).await;
                }
            }
        }
    }

    pub(super) async fn turn_interrupt(
        &mut self,
        request_id: ConnectionRequestId,
        params: TurnInterruptParams,
    ) {
        let TurnInterruptParams { thread_id, turn_id } = params;
        let Some((thread_uuid, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        self.outgoing
            .record_request_turn_id(&request_id, &turn_id)
            .await;

        // Record the pending interrupt so we can reply when TurnAborted arrives.
        {
            let thread_state = self.thread_state_manager.thread_state(thread_uuid).await;
            let mut thread_state = thread_state.lock().await;
            thread_state.pending_interrupts.push(request_id.clone());
        }

        // Submit the interrupt; we'll respond upon TurnAborted.
        let _ = self
            .submit_core_op(&request_id, thread.as_ref(), Op::Interrupt)
            .await;
    }
}

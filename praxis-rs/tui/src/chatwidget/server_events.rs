use super::*;
use praxis_protocol::protocol::DeprecationNoticeEvent;

impl ChatWidget {
    pub(crate) fn handle_server_request(
        &mut self,
        request: ServerRequest,
        replay_kind: Option<ReplayKind>,
    ) {
        let id = request.id().to_string();
        match request {
            ServerRequest::CommandExecutionRequestApproval { params, .. } => {
                self.on_exec_approval_request(id, exec_approval_request_from_params(params));
            }
            ServerRequest::FileChangeRequestApproval { params, .. } => {
                self.on_apply_patch_approval_request(
                    id,
                    patch_approval_request_from_params(params),
                );
            }
            ServerRequest::McpServerElicitationRequest { request_id, params } => {
                self.on_mcp_server_elicitation_request(
                    app_gateway_request_id_to_mcp_request_id(&request_id),
                    params,
                );
            }
            ServerRequest::PermissionsRequestApproval { params, .. } => {
                self.on_request_permissions(request_permissions_from_params(params));
            }
            ServerRequest::ToolRequestUserInput { params, .. } => {
                self.on_request_user_input(request_user_input_from_params(params));
            }
            ServerRequest::DynamicToolCall { .. }
            | ServerRequest::Cunning3dBridgeCall { .. }
            | ServerRequest::ChatgptAuthTokensRefresh { .. } => {
                if replay_kind.is_none() {
                    self.add_error_message(TUI_STUB_MESSAGE.to_string());
                }
            }
        }
    }

    pub(crate) fn handle_server_notification(
        &mut self,
        notification: ServerNotification,
        replay_kind: Option<ReplayKind>,
    ) {
        let from_replay = replay_kind.is_some();
        let is_resume_initial_replay =
            matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages));
        let is_retry_error = matches!(
            &notification,
            ServerNotification::Error(ErrorNotification {
                will_retry: true,
                ..
            })
        );
        if !is_resume_initial_replay && !is_retry_error {
            self.restore_retry_status_header_if_present();
        }
        match notification {
            ServerNotification::ThreadTokenUsageUpdated(notification) => {
                self.set_token_info(Some(token_usage_info_from_app_gateway(
                    notification.token_usage,
                )));
            }
            ServerNotification::ThreadGoalUpdated(notification) => {
                self.on_thread_goal_updated_notification(notification, replay_kind);
            }
            ServerNotification::ThreadGoalCleared(notification) => {
                self.on_thread_goal_cleared_notification(notification, replay_kind);
            }
            ServerNotification::ThreadControlChanged(notification) => {
                if self.thread_id().is_some_and(|thread_id| {
                    thread_id.to_string() == notification.thread_id.as_str()
                }) {
                    self.set_thread_control_state(notification.control_state.as_ref());
                }
            }
            ServerNotification::ThreadNameUpdated(notification) => {
                match ThreadId::from_string(&notification.thread_id) {
                    Ok(thread_id) => self.on_thread_name_updated(
                        praxis_protocol::protocol::ThreadNameUpdatedEvent {
                            thread_id,
                            thread_name: notification.thread_name,
                        },
                    ),
                    Err(err) => {
                        tracing::warn!(
                            thread_id = notification.thread_id,
                            error = %err,
                            "ignoring app-gateway ThreadNameUpdated with invalid thread_id"
                        );
                    }
                }
            }
            ServerNotification::ThreadModelChanged(notification) => {
                self.handle_thread_model_changed_notification(notification);
            }
            ServerNotification::TurnStarted(notification) => {
                self.last_non_retry_error = None;
                if !matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages)) {
                    self.apply_turn_started_context_window(notification.model_context_window);
                    self.on_task_started();
                }
            }
            ServerNotification::TurnCompleted(notification) => {
                self.handle_turn_completed_notification(notification, replay_kind);
            }
            ServerNotification::ItemStarted(notification) => {
                self.handle_item_started_notification(notification, replay_kind);
            }
            ServerNotification::ItemCompleted(notification) => {
                self.handle_item_completed_notification(notification, replay_kind);
            }
            ServerNotification::AgentMessageDelta(notification) => {
                self.on_agent_message_delta(notification.delta);
            }
            ServerNotification::PlanDelta(notification) => self.on_plan_delta(notification.delta),
            ServerNotification::ReasoningSummaryTextDelta(notification) => {
                self.on_agent_reasoning_delta(notification.delta, ReasoningBlockKind::Summary);
            }
            ServerNotification::ReasoningTextDelta(notification) => {
                self.on_agent_reasoning_delta(notification.delta, ReasoningBlockKind::Full);
            }
            ServerNotification::ReasoningSummaryPartAdded(_) => self.on_reasoning_section_break(),
            ServerNotification::TerminalInteraction(notification) => {
                self.on_terminal_interaction(TerminalInteractionEvent {
                    call_id: notification.item_id,
                    process_id: notification.process_id,
                    stdin: notification.stdin,
                })
            }
            ServerNotification::CommandExecutionOutputDelta(notification) => {
                self.on_exec_command_output_delta(ExecCommandOutputDeltaEvent {
                    call_id: notification.item_id,
                    stream: praxis_protocol::protocol::ExecOutputStream::Stdout,
                    chunk: notification.delta.into_bytes(),
                });
            }
            ServerNotification::FileChangeOutputDelta(notification) => {
                self.on_patch_apply_output_delta(notification.item_id, notification.delta);
            }
            ServerNotification::TurnDiffUpdated(notification) => {
                self.on_turn_diff(notification.diff)
            }
            ServerNotification::TurnPlanUpdated(notification) => {
                self.on_plan_update(UpdatePlanArgs {
                    explanation: notification.explanation,
                    plan: notification
                        .plan
                        .into_iter()
                        .map(|step| UpdatePlanItemArg {
                            step: step.step,
                            status: match step.status {
                                TurnPlanStepStatus::Pending => UpdatePlanItemStatus::Pending,
                                TurnPlanStepStatus::InProgress => UpdatePlanItemStatus::InProgress,
                                TurnPlanStepStatus::Completed => UpdatePlanItemStatus::Completed,
                            },
                        })
                        .collect(),
                })
            }
            ServerNotification::HookStarted(notification) => {
                self.on_hook_started(hook_started_event_from_notification(notification));
            }
            ServerNotification::HookCompleted(notification) => {
                self.on_hook_completed(hook_completed_event_from_notification(notification));
            }
            ServerNotification::Error(notification) => {
                if notification.will_retry {
                    if !from_replay {
                        self.on_stream_error(
                            notification.error.message,
                            notification.error.additional_details,
                        );
                    }
                } else {
                    self.last_non_retry_error = Some((
                        notification.turn_id.clone(),
                        notification.error.message.clone(),
                    ));
                    self.handle_non_retry_error(
                        notification.error.message,
                        notification.error.praxis_error_info,
                    );
                }
            }
            ServerNotification::SkillsChanged(_) => {
                self.submit_op(AppCommand::list_skills(
                    Vec::new(),
                    /*force_reload*/ true,
                ));
            }
            ServerNotification::ModelRerouted(_) => {}
            ServerNotification::DeprecationNotice(notification) => {
                self.on_deprecation_notice(DeprecationNoticeEvent {
                    summary: notification.summary,
                    details: notification.details,
                })
            }
            ServerNotification::ConfigWarning(notification) => self.on_warning(
                notification
                    .details
                    .map(|details| format!("{}: {details}", notification.summary))
                    .unwrap_or(notification.summary),
            ),
            ServerNotification::McpServerStatusUpdated(notification) => {
                self.on_mcp_server_status_updated(notification)
            }
            ServerNotification::ItemGuardianApprovalReviewStarted(notification) => {
                self.on_guardian_review_notification(
                    notification.target_item_id,
                    notification.turn_id,
                    notification.review,
                    notification.action,
                );
            }
            ServerNotification::ItemGuardianApprovalReviewCompleted(notification) => {
                self.on_guardian_review_notification(
                    notification.target_item_id,
                    notification.turn_id,
                    notification.review,
                    notification.action,
                );
            }
            ServerNotification::ThreadClosed(_) => {
                if !from_replay {
                    self.on_shutdown_complete();
                }
            }
            ServerNotification::ThreadRealtimeStarted(notification) => {
                if !from_replay {
                    self.on_realtime_conversation_started(
                        praxis_protocol::protocol::RealtimeConversationStartedEvent {
                            session_id: notification.session_id,
                            version: notification.version,
                        },
                    );
                }
            }
            ServerNotification::ThreadRealtimeItemAdded(notification) => {
                if !from_replay {
                    self.on_realtime_conversation_realtime(
                        praxis_protocol::protocol::RealtimeConversationRealtimeEvent {
                            payload:
                                praxis_protocol::protocol::RealtimeEvent::ConversationItemAdded(
                                    notification.item,
                                ),
                        },
                    );
                }
            }
            ServerNotification::ThreadRealtimeOutputAudioDelta(notification) => {
                if !from_replay {
                    self.on_realtime_conversation_realtime(
                        praxis_protocol::protocol::RealtimeConversationRealtimeEvent {
                            payload: praxis_protocol::protocol::RealtimeEvent::AudioOut(
                                notification.audio.into(),
                            ),
                        },
                    );
                }
            }
            ServerNotification::ThreadRealtimeError(notification) => {
                if !from_replay {
                    self.on_realtime_conversation_realtime(
                        praxis_protocol::protocol::RealtimeConversationRealtimeEvent {
                            payload: praxis_protocol::protocol::RealtimeEvent::Error(
                                notification.message,
                            ),
                        },
                    );
                }
            }
            ServerNotification::ThreadRealtimeClosed(notification) => {
                if !from_replay {
                    self.on_realtime_conversation_closed(
                        praxis_protocol::protocol::RealtimeConversationClosedEvent {
                            reason: notification.reason,
                        },
                    );
                }
            }
            ServerNotification::ServerRequestResolved(_)
            | ServerNotification::AccountUpdated(_)
            | ServerNotification::AccountRateLimitsUpdated(_)
            | ServerNotification::ThreadStarted(_)
            | ServerNotification::ThreadStatusChanged(_)
            | ServerNotification::ThreadArchived(_)
            | ServerNotification::ThreadUnarchived(_)
            | ServerNotification::RawResponseItemCompleted(_)
            | ServerNotification::CommandExecOutputDelta(_)
            | ServerNotification::McpToolCallProgress(_)
            | ServerNotification::McpServerOauthLoginCompleted(_)
            | ServerNotification::AppListUpdated(_)
            | ServerNotification::FsChanged(_)
            | ServerNotification::FuzzyFileSearchSessionUpdated(_)
            | ServerNotification::FuzzyFileSearchSessionCompleted(_)
            | ServerNotification::ThreadRealtimeTranscriptUpdated(_)
            | ServerNotification::WindowsWorldWritableWarning(_)
            | ServerNotification::WindowsSandboxSetupCompleted(_)
            | ServerNotification::AccountLoginCompleted(_) => {}
        }
    }

    pub(crate) fn handle_skills_list_response(&mut self, response: ListSkillsResponseEvent) {
        self.on_list_skills(response);
    }

    fn handle_thread_model_changed_notification(
        &mut self,
        notification: ThreadModelChangedNotification,
    ) {
        if self
            .thread_id()
            .is_some_and(|thread_id| thread_id.to_string() != notification.thread_id.as_str())
        {
            return;
        }

        let previous_model = self.current_model().to_string();
        let previous_effort = self.effective_reasoning_effort();
        let model = notification.model;
        let provider_id = notification.model_provider;
        let effort = notification.reasoning_effort;

        self.config.model_provider_id = provider_id.clone();
        self.config.model = Some(model.clone());
        if let Some(provider) = self.config.model_providers.get(&provider_id).cloned() {
            self.config.model_provider = provider;
        }
        self.current_collaboration_mode = self.current_collaboration_mode.with_updates(
            Some(model.clone()),
            Some(effort),
            /*developer_instructions*/ None,
        );
        if self.collaboration_modes_enabled()
            && let Some(mask) = self.active_collaboration_mask.as_mut()
        {
            mask.model = Some(model.clone());
            mask.reasoning_effort = Some(effort);
        }

        self.refresh_model_dependent_surfaces();
        if previous_model != model || previous_effort != effort {
            let mut message = format!("Model changed to {model}");
            if !model.starts_with("praxis-auto-") {
                let reasoning_label = Self::status_line_reasoning_effort_label(effort);
                message.push(' ');
                message.push_str(reasoning_label);
            }
            message.push('.');
            self.add_model_change_message(message);
        } else {
            self.request_redraw();
        }
    }

    pub(crate) fn handle_thread_rolled_back(&mut self) {
        self.last_copyable_output = None;
        self.pending_turn_copyable_output = None;
    }

    fn on_mcp_server_elicitation_request(
        &mut self,
        request_id: praxis_protocol::mcp::RequestId,
        params: praxis_app_gateway_protocol::McpServerElicitationRequestParams,
    ) {
        let request = praxis_protocol::approvals::ElicitationRequestEvent {
            turn_id: params.turn_id,
            server_name: params.server_name,
            id: request_id,
            request: match params.request {
                praxis_app_gateway_protocol::McpServerElicitationRequest::Form {
                    meta,
                    message,
                    requested_schema,
                } => praxis_protocol::approvals::ElicitationRequest::Form {
                    meta,
                    message,
                    requested_schema: serde_json::to_value(requested_schema)
                        .unwrap_or(serde_json::Value::Null),
                },
                praxis_app_gateway_protocol::McpServerElicitationRequest::Url {
                    meta,
                    message,
                    url,
                    elicitation_id,
                } => praxis_protocol::approvals::ElicitationRequest::Url {
                    meta,
                    message,
                    url,
                    elicitation_id,
                },
            },
        };
        self.on_elicitation_request(request);
    }

    pub(super) fn handle_turn_completed_notification(
        &mut self,
        notification: TurnCompletedNotification,
        replay_kind: Option<ReplayKind>,
    ) {
        match notification.turn.status {
            TurnStatus::Completed => {
                self.last_non_retry_error = None;
                self.on_task_complete(/*last_agent_message*/ None, replay_kind.is_some())
            }
            TurnStatus::Interrupted => {
                self.last_non_retry_error = None;
                if replay_kind.is_some() {
                    self.on_replayed_turn_aborted();
                } else {
                    self.on_interrupted_turn(TurnAbortReason::Interrupted);
                }
            }
            TurnStatus::Failed => {
                if let Some(error) = notification.turn.error {
                    if self.last_non_retry_error.as_ref()
                        == Some(&(notification.turn.id.clone(), error.message.clone()))
                    {
                        self.last_non_retry_error = None;
                    } else {
                        self.handle_non_retry_error(error.message, error.praxis_error_info);
                    }
                } else {
                    self.last_non_retry_error = None;
                    self.finalize_turn();
                    self.request_redraw();
                    self.maybe_send_next_queued_input();
                }
            }
            TurnStatus::InProgress => {}
        }
    }

    fn handle_item_started_notification(
        &mut self,
        notification: ItemStartedNotification,
        replay_kind: Option<ReplayKind>,
    ) {
        let from_replay = replay_kind.is_some();
        match notification.item {
            ThreadItem::CommandExecution {
                id,
                command,
                cwd,
                process_id,
                source,
                command_actions,
                ..
            } => {
                self.on_exec_command_begin(ExecCommandBeginEvent {
                    call_id: id,
                    process_id,
                    turn_id: notification.turn_id,
                    command: split_command_string(&command),
                    cwd,
                    parsed_cmd: command_actions
                        .into_iter()
                        .map(praxis_app_gateway_protocol::CommandAction::into_core)
                        .collect(),
                    source: source.to_core(),
                    interaction_input: None,
                });
            }
            ThreadItem::FileChange { id, changes, .. } => {
                let changes = app_gateway_patch_changes_to_core(changes);
                if matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages)) {
                    self.add_to_history(history_cell::new_patch_resume_summary(
                        changes,
                        &self.config.cwd,
                    ));
                } else {
                    self.on_patch_apply_begin(PatchApplyBeginEvent {
                        call_id: id,
                        turn_id: notification.turn_id,
                        auto_approved: false,
                        changes,
                    });
                }
            }
            ThreadItem::McpToolCall {
                id,
                server,
                tool,
                arguments,
                ..
            } => {
                self.on_mcp_tool_call_begin(McpToolCallBeginEvent {
                    call_id: id,
                    invocation: praxis_protocol::protocol::McpInvocation {
                        server,
                        tool,
                        arguments: Some(arguments),
                    },
                });
            }
            ThreadItem::WebSearch { id, .. } => {
                self.on_web_search_begin(WebSearchBeginEvent { call_id: id });
            }
            ThreadItem::ImageGeneration { id, .. } => {
                self.on_image_generation_begin(ImageGenerationBeginEvent { call_id: id });
            }
            ThreadItem::CollabAgentToolCall {
                id,
                tool,
                status,
                sender_thread_id,
                receiver_thread_ids,
                prompt,
                model,
                reasoning_effort,
                agents_states,
            } => self.on_collab_agent_tool_call(ThreadItem::CollabAgentToolCall {
                id,
                tool,
                status,
                sender_thread_id,
                receiver_thread_ids,
                prompt,
                model,
                reasoning_effort,
                agents_states,
            }),
            ThreadItem::EnteredReviewMode { review, .. } => {
                if !from_replay {
                    self.enter_review_mode_with_hint(review, /*from_replay*/ false);
                }
            }
            _ => {}
        }
    }

    fn handle_item_completed_notification(
        &mut self,
        notification: ItemCompletedNotification,
        replay_kind: Option<ReplayKind>,
    ) {
        self.handle_thread_item(
            notification.item,
            notification.turn_id,
            replay_kind.map_or(ThreadItemRenderSource::Live, ThreadItemRenderSource::Replay),
        );
    }

    fn on_patch_apply_output_delta(&mut self, _item_id: String, _delta: String) {}

    fn on_guardian_review_notification(
        &mut self,
        id: String,
        turn_id: String,
        review: praxis_app_gateway_protocol::GuardianApprovalReview,
        action: GuardianApprovalReviewAction,
    ) {
        self.on_guardian_assessment(GuardianAssessmentEvent {
            id,
            turn_id,
            status: match review.status {
                praxis_app_gateway_protocol::GuardianApprovalReviewStatus::InProgress => {
                    GuardianAssessmentStatus::InProgress
                }
                praxis_app_gateway_protocol::GuardianApprovalReviewStatus::Approved => {
                    GuardianAssessmentStatus::Approved
                }
                praxis_app_gateway_protocol::GuardianApprovalReviewStatus::Denied => {
                    GuardianAssessmentStatus::Denied
                }
                praxis_app_gateway_protocol::GuardianApprovalReviewStatus::Aborted => {
                    GuardianAssessmentStatus::Aborted
                }
            },
            risk_score: review.risk_score,
            risk_level: review.risk_level.map(|risk_level| match risk_level {
                praxis_app_gateway_protocol::GuardianRiskLevel::Low => {
                    praxis_protocol::protocol::GuardianRiskLevel::Low
                }
                praxis_app_gateway_protocol::GuardianRiskLevel::Medium => {
                    praxis_protocol::protocol::GuardianRiskLevel::Medium
                }
                praxis_app_gateway_protocol::GuardianRiskLevel::High => {
                    praxis_protocol::protocol::GuardianRiskLevel::High
                }
            }),
            rationale: review.rationale,
            action: action.into(),
        });
    }
}

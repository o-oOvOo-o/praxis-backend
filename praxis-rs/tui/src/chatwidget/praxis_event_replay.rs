use super::*;

impl ChatWidget {
    #[cfg(test)]
    pub(super) fn replay_initial_messages(&mut self, events: Vec<EventMsg>) {
        for msg in events {
            if matches!(
                msg,
                EventMsg::SessionConfigured(_) | EventMsg::ThreadNameUpdated(_)
            ) {
                continue;
            }
            // `id: None` indicates a synthetic replay id.
            self.dispatch_event_msg(
                /*id*/ None,
                msg,
                Some(ReplayKind::ResumeInitialMessages),
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn handle_praxis_event(&mut self, event: Event) {
        let Event { id, msg } = event;
        self.dispatch_event_msg(Some(id), msg, /*replay_kind*/ None);
    }

    #[cfg(test)]
    pub(crate) fn handle_praxis_event_replay(&mut self, event: Event) {
        let Event { msg, .. } = event;
        if matches!(msg, EventMsg::ShutdownComplete) {
            return;
        }
        self.dispatch_event_msg(/*id*/ None, msg, Some(ReplayKind::ThreadSnapshot));
    }

    /// Dispatch a protocol `EventMsg` to the appropriate handler.
    ///
    /// `id` is `Some` for live events and `None` for replayed events from
    /// `replay_initial_messages()`. Callers should treat `None` as a synthetic id
    /// that must not be used to correlate follow-up actions.
    #[cfg(test)]
    fn dispatch_event_msg(
        &mut self,
        id: Option<String>,
        msg: EventMsg,
        replay_kind: Option<ReplayKind>,
    ) {
        let from_replay = replay_kind.is_some();
        let is_resume_initial_replay =
            matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages));
        let is_stream_error = matches!(&msg, EventMsg::StreamError(_));
        if !is_resume_initial_replay && !is_stream_error {
            self.restore_retry_status_header_if_present();
        }

        match msg {
            EventMsg::AgentMessageDelta(_)
            | EventMsg::PlanDelta(_)
            | EventMsg::AgentReasoningDelta(_)
            | EventMsg::TerminalInteraction(_)
            | EventMsg::ExecCommandOutputDelta(_) => {}
            _ => {
                tracing::trace!("handle_praxis_event: {:?}", msg);
            }
        }

        match msg {
            EventMsg::SessionConfigured(e) => self.on_session_configured(e),
            EventMsg::ThreadNameUpdated(e) => self.on_thread_name_updated(e),
            EventMsg::AgentMessage(AgentMessageEvent { .. })
                if matches!(replay_kind, Some(ReplayKind::ThreadSnapshot))
                    && !self.is_review_mode => {}
            EventMsg::AgentMessage(AgentMessageEvent { message, .. })
                if from_replay || self.is_review_mode =>
            {
                self.on_agent_message(message)
            }
            EventMsg::AgentMessage(AgentMessageEvent { .. }) => {}
            EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
                self.on_agent_message_delta(delta)
            }
            EventMsg::PlanDelta(event) => self.on_plan_delta(event.delta),
            EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }) => {
                self.on_agent_reasoning_delta(delta, ReasoningBlockKind::Summary)
            }
            EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent {
                delta,
            }) => self.on_agent_reasoning_delta(delta, ReasoningBlockKind::Full),
            EventMsg::AgentReasoning(AgentReasoningEvent { .. }) => self.on_agent_reasoning_final(),
            EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }) => {
                self.on_agent_reasoning_delta(text, ReasoningBlockKind::Full);
                self.on_agent_reasoning_final();
            }
            EventMsg::AgentReasoningSectionBreak(_) => self.on_reasoning_section_break(),
            EventMsg::TurnStarted(event) => {
                if !is_resume_initial_replay {
                    self.apply_turn_started_context_window(event.model_context_window);
                    self.on_task_started();
                }
            }
            EventMsg::TurnComplete(TurnCompleteEvent {
                last_agent_message, ..
            }) => {
                self.on_task_complete(last_agent_message, from_replay);
            }
            EventMsg::TokenCount(ev) => {
                self.set_token_info(ev.info);
                self.on_rate_limit_snapshot(ev.rate_limits);
            }
            EventMsg::Warning(WarningEvent { message }) => self.on_warning(message),
            EventMsg::GuardianAssessment(ev) => self.on_guardian_assessment(ev),
            EventMsg::ModelReroute(_) => {}
            EventMsg::Error(ErrorEvent {
                message,
                praxis_error_info,
            }) => {
                if praxis_error_info
                    .as_ref()
                    .is_some_and(|info| self.handle_steer_rejected_error(info))
                {
                } else if let Some(kind) = praxis_error_info
                    .as_ref()
                    .and_then(core_rate_limit_error_kind)
                {
                    match kind {
                        RateLimitErrorKind::ServerOverloaded => {
                            self.on_server_overloaded_error(message)
                        }
                        RateLimitErrorKind::UsageLimit | RateLimitErrorKind::Generic => {
                            self.on_error(message)
                        }
                    }
                } else {
                    self.on_error(message);
                }
            }
            EventMsg::McpStartupUpdate(ev) => self.on_mcp_startup_update(ev),
            EventMsg::McpStartupComplete(ev) => self.on_mcp_startup_complete(ev),
            EventMsg::TurnAborted(ev) => match ev.reason {
                _ if from_replay => {
                    self.on_replayed_turn_aborted();
                }
                TurnAbortReason::Interrupted => {
                    self.on_interrupted_turn(ev.reason);
                }
                TurnAbortReason::Replaced => {
                    self.submit_pending_steers_after_interrupt = false;
                    self.pending_steers.clear();
                    self.refresh_pending_input_preview();
                    self.on_error("Turn aborted: replaced by a new task".to_owned())
                }
                TurnAbortReason::ReviewEnded => {
                    self.on_interrupted_turn(ev.reason);
                }
            },
            EventMsg::PlanUpdate(update) => self.on_plan_update(update),
            EventMsg::ThreadGoalUpdated(event) => {
                self.on_core_thread_goal_updated(event, replay_kind)
            }
            EventMsg::ExecApprovalRequest(ev) => {
                // For replayed events, synthesize an empty id (these should not occur).
                self.on_exec_approval_request(id.unwrap_or_default(), ev)
            }
            EventMsg::ApplyPatchApprovalRequest(ev) => {
                self.on_apply_patch_approval_request(id.unwrap_or_default(), ev)
            }
            EventMsg::ElicitationRequest(ev) => {
                self.on_elicitation_request(ev);
            }
            EventMsg::RequestUserInput(ev) => {
                self.on_request_user_input(ev);
            }
            EventMsg::RequestPermissions(ev) => {
                self.on_request_permissions(ev);
            }
            EventMsg::ExecCommandBegin(ev) => self.on_exec_command_begin(ev),
            EventMsg::TerminalInteraction(delta) => self.on_terminal_interaction(delta),
            EventMsg::ExecCommandOutputDelta(delta) => self.on_exec_command_output_delta(delta),
            EventMsg::PatchApplyBegin(ev) if is_resume_initial_replay => {
                self.add_to_history(history_cell::new_patch_resume_summary(
                    ev.changes,
                    &self.config.cwd,
                ));
            }
            EventMsg::PatchApplyBegin(ev) => self.on_patch_apply_begin(ev),
            EventMsg::PatchApplyEnd(ev) => self.on_patch_apply_end(ev),
            EventMsg::ExecCommandEnd(ev) => self.on_exec_command_end(ev),
            EventMsg::ViewImageToolCall(ev) => self.on_view_image_tool_call(ev),
            EventMsg::ImageGenerationBegin(ev) => self.on_image_generation_begin(ev),
            EventMsg::ImageGenerationEnd(ev) => self.on_image_generation_end(ev),
            EventMsg::McpToolCallBegin(ev) => self.on_mcp_tool_call_begin(ev),
            EventMsg::McpToolCallEnd(ev) => self.on_mcp_tool_call_end(ev),
            EventMsg::WebSearchBegin(ev) => self.on_web_search_begin(ev),
            EventMsg::WebSearchEnd(ev) => self.on_web_search_end(ev),
            EventMsg::GetHistoryEntryResponse(ev) => self.handle_history_entry_response(ev),
            EventMsg::McpListToolsResponse(ev) => self.on_list_mcp_tools(ev),
            EventMsg::ListSkillsResponse(ev) => self.on_list_skills(ev),
            EventMsg::SkillsUpdateAvailable => {
                self.submit_op(AppCommand::list_skills(
                    Vec::new(),
                    /*force_reload*/ true,
                ));
            }
            EventMsg::ShutdownComplete => self.on_shutdown_complete(),
            EventMsg::TurnDiff(TurnDiffEvent { unified_diff }) => self.on_turn_diff(unified_diff),
            EventMsg::DeprecationNotice(ev) => self.on_deprecation_notice(ev),
            EventMsg::BackgroundEvent(BackgroundEventEvent { message }) => {
                self.on_background_event(message)
            }
            EventMsg::UndoStarted(ev) => self.on_undo_started(ev),
            EventMsg::UndoCompleted(ev) => self.on_undo_completed(ev),
            EventMsg::StreamError(StreamErrorEvent {
                message,
                additional_details,
                ..
            }) => {
                if !is_resume_initial_replay {
                    self.on_stream_error(message, additional_details);
                }
            }
            EventMsg::UserMessage(ev) => {
                if from_replay || self.should_render_realtime_user_message_event(&ev) {
                    self.on_user_message_event(ev);
                }
            }
            EventMsg::EnteredReviewMode(review_request) => {
                self.on_entered_review_mode(review_request, from_replay)
            }
            EventMsg::ExitedReviewMode(review) => self.on_exited_review_mode(review),
            EventMsg::ContextCompacted(_) => self.on_agent_message("Context compacted".to_owned()),
            EventMsg::CollabAgentSpawnBegin(CollabAgentSpawnBeginEvent {
                call_id,
                model,
                reasoning_effort,
                ..
            }) => {
                self.pending_collab_spawn_requests.insert(
                    call_id,
                    multi_agents::SpawnRequestSummary {
                        model,
                        reasoning_effort,
                    },
                );
            }
            EventMsg::CollabAgentSpawnEnd(ev) => {
                let spawn_request = self.pending_collab_spawn_requests.remove(&ev.call_id);
                self.on_collab_event(multi_agents::spawn_end(ev, spawn_request.as_ref()));
            }
            EventMsg::CollabAgentInteractionBegin(_) => {}
            EventMsg::CollabAgentInteractionEnd(ev) => {
                self.on_collab_event(multi_agents::interaction_end(ev))
            }
            EventMsg::CollabWaitingBegin(ev) => {
                self.on_collab_event(multi_agents::waiting_begin(ev))
            }
            EventMsg::CollabWaitingEnd(ev) => self.on_collab_event(multi_agents::waiting_end(ev)),
            EventMsg::CollabCloseBegin(_) => {}
            EventMsg::CollabCloseEnd(ev) => self.on_collab_event(multi_agents::close_end(ev)),
            EventMsg::CollabResumeBegin(ev) => self.on_collab_event(multi_agents::resume_begin(ev)),
            EventMsg::CollabResumeEnd(ev) => self.on_collab_event(multi_agents::resume_end(ev)),
            EventMsg::ThreadRolledBack(rollback) => {
                // Conservatively clear `/copy` state on rollback. The app layer trims visible
                // transcript cells, but we do not maintain rollback-aware raw-markdown history yet,
                // so keeping the previous cache can return content that was just removed.
                self.last_copyable_output = None;
                self.pending_turn_copyable_output = None;
                if from_replay {
                    self.app_event_tx.send(AppEvent::ApplyThreadRollback {
                        num_turns: rollback.num_turns,
                    });
                }
            }
            EventMsg::RawResponseItem(_)
            | EventMsg::ItemStarted(_)
            | EventMsg::AgentMessageContentDelta(_)
            | EventMsg::ReasoningContentDelta(_)
            | EventMsg::ReasoningRawContentDelta(_)
            | EventMsg::DynamicToolCallRequest(_)
            | EventMsg::DynamicToolCallResponse(_) => {}
            EventMsg::HookStarted(event) => self.on_hook_started(event),
            EventMsg::HookCompleted(event) => self.on_hook_completed(event),
            EventMsg::RealtimeConversationStarted(ev) => {
                if !from_replay {
                    self.on_realtime_conversation_started(ev);
                }
            }
            EventMsg::RealtimeConversationRealtime(ev) => {
                if !from_replay {
                    self.on_realtime_conversation_realtime(ev);
                }
            }
            EventMsg::RealtimeConversationClosed(ev) => {
                if !from_replay {
                    self.on_realtime_conversation_closed(ev);
                }
            }
            EventMsg::ItemCompleted(event) => {
                let item = event.item;
                if !from_replay && let praxis_protocol::items::TurnItem::UserMessage(item) = &item {
                    let event = item.to_user_message_event();
                    let rendered = Self::rendered_user_message_event_from_event(&event);
                    let compare_key = Self::pending_steer_compare_key_from_item(item);
                    if self
                        .pending_steers
                        .front()
                        .is_some_and(|pending| pending.compare_key == compare_key)
                    {
                        if let Some(pending) = self.pending_steers.pop_front() {
                            self.refresh_pending_input_preview();
                            let pending_event = UserMessageEvent {
                                message: pending.user_message.text,
                                images: Some(pending.user_message.remote_image_urls),
                                local_images: pending
                                    .user_message
                                    .local_images
                                    .into_iter()
                                    .map(|image| image.path)
                                    .collect(),
                                text_elements: pending.user_message.text_elements,
                            };
                            self.on_user_message_event(pending_event);
                        } else if self.last_rendered_user_message_event.as_ref() != Some(&rendered)
                        {
                            tracing::warn!(
                                "pending steer matched compare key but queue was empty when rendering committed user message"
                            );
                            self.on_user_message_event(event);
                        }
                    } else if self.last_rendered_user_message_event.as_ref() != Some(&rendered) {
                        self.on_user_message_event(event);
                    }
                }
                if let praxis_protocol::items::TurnItem::Plan(plan_item) = &item {
                    self.on_plan_item_completed(plan_item.text.clone());
                }
                if let praxis_protocol::items::TurnItem::AgentMessage(item) = item {
                    self.on_agent_message_item_completed(item);
                }
            }
        }

        if !from_replay && self.agent_turn_running {
            self.refresh_runtime_metrics();
        }
    }

    pub(super) fn enter_review_mode_with_hint(&mut self, hint: String, from_replay: bool) {
        if self.pre_review_token_info.is_none() {
            self.pre_review_token_info = Some(self.token_info.clone());
        }
        if !from_replay && !self.bottom_pane.is_task_running() {
            self.bottom_pane.set_task_running(/*running*/ true);
        }
        self.is_review_mode = true;
        let banner = format!(">> Code review started: {hint} <<");
        self.add_to_history(history_cell::new_review_status_line(banner));
        self.request_redraw();
    }

    pub(super) fn exit_review_mode_after_item(&mut self) {
        self.flush_answer_stream_with_separator();
        self.flush_interrupt_queue();
        self.flush_active_cell();
        self.is_review_mode = false;
        self.restore_pre_review_token_info();
        self.add_to_history(history_cell::new_review_status_line(
            "<< Code review finished >>".to_string(),
        ));
        self.request_redraw();
    }

    #[cfg(test)]
    fn on_entered_review_mode(&mut self, review: ReviewRequest, from_replay: bool) {
        let hint = review
            .user_facing_hint
            .unwrap_or_else(|| praxis_core::review_prompts::user_facing_hint(&review.target));
        self.enter_review_mode_with_hint(hint, from_replay);
    }

    #[cfg(test)]
    fn on_exited_review_mode(&mut self, review: ExitedReviewModeEvent) {
        if let Some(output) = review.review_output {
            self.flush_answer_stream_with_separator();
            self.flush_interrupt_queue();
            self.flush_active_cell();

            if output.findings.is_empty() {
                let explanation = output.overall_explanation.trim().to_string();
                if explanation.is_empty() {
                    tracing::error!("Reviewer failed to output a response.");
                    self.add_to_history(history_cell::new_error_event(
                        "Reviewer failed to output a response.".to_owned(),
                    ));
                } else {
                    // Show explanation when there are no structured findings.
                    let mut rendered: Vec<ratatui::text::Line<'static>> = vec!["".into()];
                    append_markdown(
                        &explanation,
                        /*width*/ None,
                        Some(self.config.cwd.as_path()),
                        &mut rendered,
                    );
                    let body_cell = AgentMessageCell::new(rendered, /*is_first_line*/ false);
                    self.app_event_tx
                        .send(AppEvent::InsertHistoryCell(Box::new(body_cell)));
                }
            }
            // Final message is rendered as part of the AgentMessage.
        }
        self.exit_review_mode_after_item();
    }

    pub(super) fn on_user_message_event(&mut self, event: UserMessageEvent) {
        self.last_rendered_user_message_event =
            Some(Self::rendered_user_message_event_from_event(&event));
        let remote_image_urls = event.images.unwrap_or_default();
        if !event.message.trim().is_empty()
            || !event.text_elements.is_empty()
            || !remote_image_urls.is_empty()
        {
            self.add_to_history(history_cell::new_user_prompt(
                event.message,
                event.text_elements,
                event.local_images,
                remote_image_urls,
            ));
        }

        // User messages reset separator state so the next agent response doesn't add a stray break.
        self.needs_final_message_separator = false;
    }
}

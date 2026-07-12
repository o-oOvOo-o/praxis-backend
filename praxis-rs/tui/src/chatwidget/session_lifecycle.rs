use super::*;

impl ChatWidget {
    // --- Small event handlers ---
    pub(super) fn on_session_configured(
        &mut self,
        event: praxis_protocol::protocol::SessionConfiguredEvent,
    ) {
        if self
            .thread_id
            .is_some_and(|thread_id| thread_id != event.session_id)
        {
            self.work_panel.clear_thread_projection();
        }
        self.bottom_pane
            .set_history_metadata(event.history_log_id, event.history_entry_count);
        self.set_skills(/*skills*/ None);
        self.session_network_proxy = event.network_proxy.clone();
        self.thread_id = Some(event.session_id);
        self.thread_name = event.thread_name.clone();
        self.forked_from = event.forked_from_id;
        self.current_rollout_path = event.rollout_path.clone();
        self.current_cwd = Some(event.cwd.clone());
        self.refresh_rendered_status_state();
        match AbsolutePathBuf::try_from(event.cwd.clone()) {
            Ok(cwd) => self.config.cwd = cwd,
            Err(err) => {
                tracing::warn!(path = %event.cwd.display(), %err, "session cwd should be absolute");
            }
        }
        if let Err(err) = self
            .config
            .permissions
            .approval_policy
            .set(event.approval_policy)
        {
            tracing::warn!(%err, "failed to sync approval_policy from SessionConfigured");
            self.config.permissions.approval_policy =
                Constrained::allow_only(event.approval_policy);
        }
        if let Err(err) = self
            .config
            .permissions
            .sandbox_policy
            .set(event.sandbox_policy.clone())
        {
            tracing::warn!(%err, "failed to sync sandbox_policy from SessionConfigured");
            self.config.permissions.sandbox_policy =
                Constrained::allow_only(event.sandbox_policy.clone());
        }
        self.config.approvals_reviewer = event.approvals_reviewer;
        self.status_line_project_root_name_cache = None;
        self.last_copyable_output = None;
        self.pending_turn_copyable_output = None;
        let forked_from_id = event.forked_from_id;
        let forked_from_name = event
            .thread_name
            .clone()
            .filter(|name| !name.trim().is_empty());
        let model_for_header = event.model.clone();
        self.session_header.set_model(&model_for_header);
        self.current_collaboration_mode = self.current_collaboration_mode.with_updates(
            Some(model_for_header.clone()),
            Some(event.reasoning_effort.clone()),
            /*developer_instructions*/ None,
        );
        if let Some(mask) = self.active_collaboration_mask.as_mut() {
            mask.model = Some(model_for_header.clone());
            mask.reasoning_effort = Some(event.reasoning_effort.clone());
        }
        self.refresh_model_display();
        self.refresh_status_surfaces();
        self.sync_fast_command_enabled();
        self.sync_personality_command_enabled();
        self.sync_plugins_command_enabled();
        self.refresh_plugin_mentions();
        let startup_tooltip_override = self.startup_tooltip_override.take();
        let show_fast_status = self.should_show_fast_status(&model_for_header, event.service_tier);
        #[cfg(test)]
        let initial_messages = event.initial_messages.clone();
        let session_info_cell = history_cell::new_session_info(
            &self.config,
            &self.tui_config,
            &model_for_header,
            event,
            self.show_welcome_banner,
            startup_tooltip_override,
            self.plan_type,
            show_fast_status,
        );
        self.apply_session_info_cell(session_info_cell);

        #[cfg(test)]
        if let Some(messages) = initial_messages {
            self.replay_initial_messages(messages);
        }
        self.submit_op(AppCommand::list_skills(
            Vec::new(),
            /*force_reload*/ true,
        ));
        if self.connectors_enabled() {
            self.prefetch_connectors();
        }
        if let Some(user_message) = self.initial_user_message.take() {
            if self.suppress_initial_user_message_submit {
                self.initial_user_message = Some(user_message);
            } else {
                self.submit_user_message(user_message);
            }
        }
        if let Some(forked_from_id) = forked_from_id {
            self.emit_forked_thread_event(forked_from_id, forked_from_name);
        }
        if !self.suppress_session_configured_redraw {
            self.request_redraw();
        }
    }

    pub(crate) fn set_initial_user_message_submit_suppressed(&mut self, suppressed: bool) {
        self.suppress_initial_user_message_submit = suppressed;
    }

    pub(crate) fn submit_initial_user_message_if_pending(&mut self) {
        if let Some(user_message) = self.initial_user_message.take() {
            self.submit_user_message(user_message);
        }
    }

    pub(crate) fn handle_thread_session(&mut self, session: ThreadSessionState) {
        if self.selfwork_plan_path != session.selfwork_plan_path {
            self.selfwork_last_plan_digest = None;
            self.selfwork_stall_count = 0;
            self.selfwork_turn_in_flight = false;
        }
        self.selfwork_plan_path = session.selfwork_plan_path.clone();
        self.sync_work_panel_selfwork();
        self.on_session_configured(session_state_to_configured_event(session));
    }

    pub(crate) fn maybe_resume_selfwork_if_idle(&mut self) {
        self.maybe_start_selfwork_turn_now();
    }

    pub(super) fn emit_forked_thread_event(
        &self,
        forked_from_id: ThreadId,
        forked_from_name: Option<String>,
    ) {
        let app_event_tx = self.app_event_tx.clone();
        let forked_from_id_text = forked_from_id.to_string();
        let line = match forked_from_name {
            Some(name) => vec![
                "• ".dim(),
                "Thread forked from ".into(),
                name.cyan(),
                " (".into(),
                forked_from_id_text.cyan(),
                ")".into(),
            ],
            None => vec![
                "• ".dim(),
                "Thread forked from ".into(),
                forked_from_id_text.cyan(),
            ],
        }
        .into();
        app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
            PlainHistoryCell::new(vec![line]),
        )));
    }

    pub(super) fn on_thread_name_updated(
        &mut self,
        event: praxis_protocol::protocol::ThreadNameUpdatedEvent,
    ) {
        if self.thread_id == Some(event.thread_id) {
            self.thread_name = event.thread_name;
            self.refresh_terminal_title();
            self.request_redraw();
        }
    }

    pub(super) fn on_task_started(&mut self) {
        self.agent_turn_running = true;
        self.turn_sleep_inhibitor
            .set_turn_running(/*turn_running*/ true);
        self.saw_plan_update_this_turn = false;
        self.saw_plan_item_this_turn = false;
        self.last_plan_progress = None;
        self.plan_delta_buffer.clear();
        self.plan_item_active = false;
        self.adaptive_chunking.reset();
        self.plan_stream_controller = None;
        self.pending_turn_copyable_output = None;
        self.turn_runtime_metrics = RuntimeMetricsSummary::default();
        self.session_telemetry.reset_runtime_metrics();
        self.bottom_pane.clear_quit_shortcut_hint();
        self.quit_shortcut_expires_at = None;
        self.quit_shortcut_key = None;
        self.update_task_running_state();
        self.retry_status_header = None;
        self.pending_status_indicator_restore = false;
        self.bottom_pane
            .set_interrupt_hint_visible(/*visible*/ true);
        self.terminal_title_status_kind = TerminalTitleStatusKind::TurnRunning;
        self.set_status_header(GENERIC_STATUS_HEADER.to_string());
        self.full_reasoning_buffer.clear();
        self.reasoning_buffer.clear();
        self.reasoning_block_kind = None;
        self.request_redraw();
    }

    pub(super) fn on_task_complete(
        &mut self,
        last_agent_message: Option<String>,
        from_replay: bool,
    ) {
        let completed_selfwork_turn = self.selfwork_turn_in_flight;
        self.selfwork_turn_in_flight = false;
        self.sync_work_panel_selfwork();
        self.submit_pending_steers_after_interrupt = false;
        let copyable_turn_output = last_agent_message
            .filter(|message| !message.trim().is_empty())
            .or_else(|| self.pending_turn_copyable_output.take());
        if let Some(message) = copyable_turn_output.as_ref() {
            self.last_copyable_output = Some(message.clone());
        }
        // If a stream is currently active, finalize it.
        self.flush_answer_stream_with_separator();
        if let Some(mut controller) = self.plan_stream_controller.take()
            && let Some(cell) = controller.finalize()
        {
            self.add_boxed_history(cell);
        }
        self.flush_unified_exec_wait_streak();
        if !from_replay {
            self.collect_runtime_metrics_delta();
            let runtime_metrics =
                (!self.turn_runtime_metrics.is_empty()).then_some(self.turn_runtime_metrics);
            let show_work_separator = self.needs_final_message_separator && self.had_work_activity;
            if show_work_separator || runtime_metrics.is_some() {
                let elapsed_seconds = if show_work_separator {
                    self.bottom_pane
                        .status_widget()
                        .map(crate::status_indicator_widget::StatusIndicatorWidget::elapsed_seconds)
                        .map(|current| self.worked_elapsed_from(current))
                } else {
                    None
                };
                self.add_to_history(history_cell::FinalMessageSeparator::new(
                    elapsed_seconds,
                    runtime_metrics,
                ));
            }
            self.turn_runtime_metrics = RuntimeMetricsSummary::default();
            self.needs_final_message_separator = false;
            self.had_work_activity = false;
            self.request_status_line_branch_refresh();
        }
        self.flush_pending_goal_completion_notice();
        // Mark task stopped and request redraw now that all content is in history.
        self.pending_status_indicator_restore = false;
        self.agent_turn_running = false;
        self.turn_sleep_inhibitor
            .set_turn_running(/*turn_running*/ false);
        self.update_task_running_state();
        self.running_commands.clear();
        self.suppressed_exec_calls.clear();
        self.last_unified_wait = None;
        self.unified_exec_wait_streak = None;
        self.request_redraw();

        let had_pending_steers = !self.pending_steers.is_empty();
        self.refresh_pending_input_preview();

        if !from_replay && !self.has_queued_follow_up_messages() && !had_pending_steers {
            self.maybe_prompt_plan_implementation();
        }
        // Keep this flag for replayed completion events so a subsequent live TurnComplete can
        // still show the prompt once after thread switch replay.
        if !from_replay {
            self.saw_plan_item_this_turn = false;
        }
        // If there is a queued user message, send exactly one now to begin the next turn.
        self.maybe_send_next_queued_input();
        if !from_replay
            && !self.bottom_pane.is_task_running()
            && !self.has_queued_follow_up_messages()
        {
            self.maybe_continue_selfwork_after_turn(completed_selfwork_turn);
        }
        // Emit a notification when the turn completes (suppressed if focused).
        self.notify(Notification::AgentTurnComplete {
            response: copyable_turn_output.unwrap_or_default(),
        });

        self.maybe_show_pending_rate_limit_prompt();
    }

    pub(super) fn on_thread_goal_updated_notification(
        &mut self,
        notification: praxis_app_gateway_protocol::ThreadGoalUpdatedNotification,
        replay_kind: Option<ReplayKind>,
    ) {
        if matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages)) {
            return;
        }
        self.work_panel
            .set_goal(Self::work_panel_goal_from_app_gateway(&notification.goal));
        if notification.goal.status != praxis_app_gateway_protocol::ThreadGoalStatus::Complete {
            return;
        }
        self.pending_goal_completion_elapsed =
            Some(format_goal_elapsed(notification.goal.time_used_seconds));
    }

    pub(super) fn on_thread_goal_cleared_notification(
        &mut self,
        notification: ThreadGoalClearedNotification,
        replay_kind: Option<ReplayKind>,
    ) {
        if matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages)) {
            return;
        }
        match ThreadId::from_string(&notification.thread_id) {
            Ok(thread_id) => self.on_thread_goal_cleared(thread_id),
            Err(err) => {
                tracing::warn!(
                    thread_id = notification.thread_id,
                    error = %err,
                    "ignoring app-gateway ThreadGoalCleared with invalid thread_id"
                );
            }
        }
    }

    pub(crate) fn on_thread_goal_cleared(&mut self, thread_id: ThreadId) {
        if self.thread_id != Some(thread_id) {
            return;
        }
        self.work_panel.clear_goal();
        self.pending_goal_completion_elapsed = None;
        self.request_redraw();
    }

    pub(crate) fn show_goal_summary(&mut self, goal: &ThreadGoal) {
        let mut lines = vec![
            format!("Goal: {}", app_gateway_goal_status_label(goal.status)),
            format!("Objective: {}", goal.objective),
            format!("Time used: {}", format_goal_elapsed(goal.time_used_seconds)),
            format!("Tokens used: {}", format_tokens_compact(goal.tokens_used)),
        ];
        if let Some(token_budget) = goal.token_budget {
            lines.push(format!(
                "Token budget: {}",
                format_tokens_compact(token_budget)
            ));
        }
        self.add_info_message(
            lines.join("\n"),
            Some("Commands: /goal edit, /goal pause, /goal resume, /goal clear".to_string()),
        );
    }

    pub(crate) fn show_goal_edit_prompt(&mut self, thread_id: ThreadId, goal: ThreadGoal) {
        let tx = self.app_event_tx.clone();
        let status = edited_goal_status(goal.status);
        let token_budget = goal.token_budget;
        let view = CustomPromptView::new_with_initial_text(
            "Edit goal".to_string(),
            "Type a goal objective and press Enter".to_string(),
            /*context_label*/ None,
            goal.objective,
            Box::new(move |objective| {
                tx.send(AppEvent::SetThreadGoalObjective {
                    thread_id,
                    objective,
                    mode: ThreadGoalSetMode::UpdateExisting {
                        status,
                        token_budget,
                    },
                });
            }),
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    #[cfg(test)]
    pub(super) fn on_core_thread_goal_updated(
        &mut self,
        event: praxis_protocol::protocol::ThreadGoalUpdatedEvent,
        replay_kind: Option<ReplayKind>,
    ) {
        if matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages)) {
            return;
        }
        self.work_panel
            .set_goal(Self::work_panel_goal_from_core(&event.goal));
        if event.goal.status != praxis_protocol::protocol::ThreadGoalStatus::Complete {
            return;
        }
        self.pending_goal_completion_elapsed =
            Some(format_goal_elapsed(event.goal.time_used_seconds));
    }

    pub(super) fn work_panel_goal_from_app_gateway(
        goal: &praxis_app_gateway_protocol::ThreadGoal,
    ) -> WorkPanelGoalState {
        WorkPanelGoalState {
            status: match goal.status {
                praxis_app_gateway_protocol::ThreadGoalStatus::Active => {
                    WorkPanelGoalStatus::Active
                }
                praxis_app_gateway_protocol::ThreadGoalStatus::Paused => {
                    WorkPanelGoalStatus::Paused
                }
                praxis_app_gateway_protocol::ThreadGoalStatus::Blocked => {
                    WorkPanelGoalStatus::Blocked
                }
                praxis_app_gateway_protocol::ThreadGoalStatus::UsageLimited => {
                    WorkPanelGoalStatus::UsageLimited
                }
                praxis_app_gateway_protocol::ThreadGoalStatus::BudgetLimited => {
                    WorkPanelGoalStatus::BudgetLimited
                }
                praxis_app_gateway_protocol::ThreadGoalStatus::Complete => {
                    WorkPanelGoalStatus::Complete
                }
            },
            objective: goal.objective.clone(),
            elapsed: (goal.time_used_seconds > 0)
                .then(|| format_goal_elapsed(goal.time_used_seconds)),
            token_budget: goal.token_budget,
            tokens_used: goal.tokens_used,
        }
    }

    #[cfg(test)]
    pub(super) fn work_panel_goal_from_core(
        goal: &praxis_protocol::protocol::ThreadGoal,
    ) -> WorkPanelGoalState {
        WorkPanelGoalState {
            status: match goal.status {
                praxis_protocol::protocol::ThreadGoalStatus::Active => WorkPanelGoalStatus::Active,
                praxis_protocol::protocol::ThreadGoalStatus::Paused => WorkPanelGoalStatus::Paused,
                praxis_protocol::protocol::ThreadGoalStatus::Blocked => {
                    WorkPanelGoalStatus::Blocked
                }
                praxis_protocol::protocol::ThreadGoalStatus::UsageLimited => {
                    WorkPanelGoalStatus::UsageLimited
                }
                praxis_protocol::protocol::ThreadGoalStatus::BudgetLimited => {
                    WorkPanelGoalStatus::BudgetLimited
                }
                praxis_protocol::protocol::ThreadGoalStatus::Complete => {
                    WorkPanelGoalStatus::Complete
                }
            },
            objective: goal.objective.clone(),
            elapsed: (goal.time_used_seconds > 0)
                .then(|| format_goal_elapsed(goal.time_used_seconds)),
            token_budget: goal.token_budget,
            tokens_used: goal.tokens_used,
        }
    }

    pub(super) fn flush_pending_goal_completion_notice(&mut self) {
        let Some(elapsed) = self.pending_goal_completion_elapsed.take() else {
            return;
        };
        self.add_info_message(format!("Goal complete - 总计耗时 {elapsed}"), None);
    }

    pub(super) fn maybe_prompt_plan_implementation(&mut self) {
        if !self.collaboration_modes_enabled() {
            return;
        }
        if self.has_queued_follow_up_messages() {
            return;
        }
        if self.active_mode_kind() != ModeKind::Plan {
            return;
        }
        if !self.saw_plan_item_this_turn {
            return;
        }
        if !self.bottom_pane.no_modal_or_popup_active() {
            return;
        }

        if matches!(
            self.rate_limit_switch_prompt,
            RateLimitSwitchPromptState::Pending
        ) {
            return;
        }

        self.open_plan_implementation_prompt();
    }

    pub(super) fn open_plan_implementation_prompt(&mut self) {
        let default_mask = collaboration_modes::default_mode_mask(self.model_catalog.as_ref());
        let (implement_actions, implement_disabled_reason) = match default_mask {
            Some(mask) => {
                let user_text = PLAN_IMPLEMENTATION_CODING_MESSAGE.to_string();
                let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                    tx.send(AppEvent::SubmitUserMessageWithMode {
                        text: user_text.clone(),
                        collaboration_mode: mask.clone(),
                    });
                })];
                (actions, None)
            }
            None => (Vec::new(), Some("Default mode unavailable".to_string())),
        };
        let items = vec![
            SelectionItem {
                name: PLAN_IMPLEMENTATION_YES.to_string(),
                description: Some("Switch to Default and start coding.".to_string()),
                selected_description: None,
                is_current: false,
                actions: implement_actions,
                disabled_reason: implement_disabled_reason,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: PLAN_IMPLEMENTATION_NO.to_string(),
                description: Some("Continue planning with the model.".to_string()),
                selected_description: None,
                is_current: false,
                actions: Vec::new(),
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(PLAN_IMPLEMENTATION_TITLE.to_string()),
            subtitle: None,
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
        self.notify(Notification::PlanModePrompt {
            title: PLAN_IMPLEMENTATION_TITLE.to_string(),
        });
    }

    pub(super) fn has_queued_follow_up_messages(&self) -> bool {
        !self.rejected_steers_queue.is_empty() || !self.queued_user_messages.is_empty()
    }

    pub(super) fn pop_next_queued_user_message(&mut self) -> Option<UserMessage> {
        if self.rejected_steers_queue.is_empty() {
            self.queued_user_messages.pop_front()
        } else {
            Some(merge_user_messages(
                self.rejected_steers_queue.drain(..).collect(),
            ))
        }
    }

    pub(super) fn pop_latest_queued_user_message(&mut self) -> Option<UserMessage> {
        self.queued_user_messages
            .pop_back()
            .or_else(|| self.rejected_steers_queue.pop_back())
    }

    pub(crate) fn enqueue_rejected_steer(&mut self) -> bool {
        let Some(pending_steer) = self.pending_steers.pop_front() else {
            tracing::warn!(
                "received active-turn-not-steerable error without a matching pending steer"
            );
            return false;
        };
        self.rejected_steers_queue
            .push_back(pending_steer.user_message);
        self.refresh_pending_input_preview();
        true
    }

    #[cfg(test)]
    pub(super) fn handle_steer_rejected_error(
        &mut self,
        praxis_error_info: &CorePraxisErrorInfo,
    ) -> bool {
        matches!(
            praxis_error_info,
            CorePraxisErrorInfo::ActiveTurnNotSteerable { .. }
        ) && self.enqueue_rejected_steer()
    }

    pub(super) fn handle_app_gateway_steer_rejected_error(
        &mut self,
        praxis_error_info: &AppGatewayPraxisErrorInfo,
    ) -> bool {
        matches!(
            praxis_error_info,
            AppGatewayPraxisErrorInfo::ActiveTurnNotSteerable { .. }
        ) && self.enqueue_rejected_steer()
    }
}

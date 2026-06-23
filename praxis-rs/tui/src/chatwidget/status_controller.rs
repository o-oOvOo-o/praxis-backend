use super::*;

fn has_websocket_timing_metrics(summary: RuntimeMetricsSummary) -> bool {
    summary.responses_api_overhead_ms > 0
        || summary.responses_api_inference_time_ms > 0
        || summary.responses_api_engine_iapi_ttft_ms > 0
        || summary.responses_api_engine_service_ttft_ms > 0
        || summary.responses_api_engine_iapi_tbt_ms > 0
        || summary.responses_api_engine_service_tbt_ms > 0
}

impl ChatWidget {
    ///
    /// The bottom pane only has one running flag, but this module treats it as a derived state of
    /// both the agent turn lifecycle and MCP startup lifecycle.
    pub(super) fn update_task_running_state(&mut self) {
        let was_running = self.bottom_pane.is_task_running();
        let is_interruptible = self.agent_turn_running || self.mcp_startup_status.is_some();
        let is_running = is_interruptible;
        if was_running != is_running {
            self.clear_status_activity();
        }
        self.bottom_pane.set_task_running(is_running);
        if is_running {
            self.bottom_pane
                .set_interrupt_hint_visible(is_interruptible);
        }
        self.sync_work_panel_live_status();
        self.refresh_terminal_title();
    }

    pub(super) fn clear_status_activity(&mut self) {
        self.turn_status_snapshot.clear_activity();
        self.sync_status_activity_message();
    }

    pub(super) fn push_status_activity(&mut self, summary: impl Into<String>) {
        let summary = summary.into();
        let normalized = summary.trim();
        if normalized.is_empty() {
            return;
        }

        let normalized = truncate_text(normalized, STATUS_ACTIVITY_TEXT_MAX_GRAPHEMES);
        if !self.turn_status_snapshot.push_activity(normalized) {
            return;
        }
        self.sync_status_activity_message();
    }

    pub(super) fn sync_status_activity_message(&mut self) {
        self.bottom_pane
            .set_status_activity_message(self.turn_status_snapshot.activity_summary());
        self.sync_work_panel_live_status();
    }

    pub(super) fn sync_work_panel_live_status(&mut self) {
        if self.bottom_pane.is_task_running() {
            self.work_panel.set_live_status(
                self.current_status.header.clone(),
                self.current_status.details.clone(),
                self.turn_status_snapshot.activity_summary(),
            );
        } else {
            self.work_panel.clear_live_status();
        }
    }

    pub(super) fn sync_work_panel_context(&mut self) {
        self.work_panel.set_context(
            self.status_budget_message()
                .map(|message| WorkPanelContextState { message }),
        );
    }

    pub(super) fn sync_work_panel_queue(&mut self) {
        self.work_panel.set_queue(WorkPanelQueueState {
            queued_messages: self.queued_user_messages.len(),
            pending_steers: self.pending_steers.len(),
            rejected_steers: self.rejected_steers_queue.len(),
            pending_approvals: self.pending_thread_approvals_count,
        });
    }

    pub(super) fn sync_work_panel_control(&mut self) {
        let control =
            self.thread_control_state
                .as_ref()
                .map(|control_state| WorkPanelControlState {
                    label: thread_control_display_label(control_state),
                    read_only: control_state.read_only,
                });
        self.work_panel.set_control(control);
    }

    pub(super) fn restore_reasoning_status_header(&mut self) {
        if let Some(header) = extract_first_bold(&self.reasoning_buffer) {
            self.terminal_title_status_kind = TerminalTitleStatusKind::Reasoning;
            let kind = self
                .reasoning_block_kind
                .unwrap_or(ReasoningBlockKind::Summary);
            self.set_status_with_thinking_persona(
                header,
                /*details*/ None,
                StatusDetailsCapitalization::CapitalizeFirst,
                STATUS_DETAILS_DEFAULT_MAX_LINES,
                self.thinking_persona_for_reasoning_kind(kind),
            );
        } else if self.bottom_pane.is_task_running() {
            self.terminal_title_status_kind = TerminalTitleStatusKind::TurnRunning;
            self.set_status_header(GENERIC_STATUS_HEADER.to_string());
        }
    }

    pub(super) fn flush_unified_exec_wait_streak(&mut self) {
        let Some(wait) = self.unified_exec_wait_streak.take() else {
            return;
        };
        self.needs_final_message_separator = true;
        let cell = history_cell::new_unified_exec_interaction(wait.command_display, String::new());
        self.app_event_tx
            .send(AppEvent::InsertHistoryCell(Box::new(cell)));
        self.restore_reasoning_status_header();
    }

    pub(super) fn flush_answer_stream_with_separator(&mut self) {
        if let Some(mut controller) = self.stream_controller.take()
            && let Some(cell) = controller.finalize()
        {
            self.add_boxed_history(cell);
        }
        self.adaptive_chunking.reset();
    }

    pub(super) fn stream_controllers_idle(&self) -> bool {
        self.stream_controller
            .as_ref()
            .map(|controller| controller.queued_lines() == 0)
            .unwrap_or(true)
            && self
                .plan_stream_controller
                .as_ref()
                .map(|controller| controller.queued_lines() == 0)
                .unwrap_or(true)
    }

    /// Restore the status indicator only after commentary completion is pending,
    /// the turn is still running, and all stream queues have drained.
    ///
    /// This gate prevents flicker while normal output is still actively
    /// streaming, but still restores a visible "working" affordance when a
    /// commentary block ends before the turn itself has completed.
    pub(super) fn maybe_restore_status_indicator_after_stream_idle(&mut self) {
        if !self.pending_status_indicator_restore
            || !self.bottom_pane.is_task_running()
            || !self.stream_controllers_idle()
        {
            return;
        }

        self.bottom_pane.ensure_status_indicator();
        self.refresh_rendered_status_state();
        self.pending_status_indicator_restore = false;
    }

    /// Update the status indicator header and details.
    ///
    /// Passing `None` clears any existing details.
    pub(super) fn set_status(
        &mut self,
        header: String,
        details: Option<String>,
        details_capitalization: StatusDetailsCapitalization,
        details_max_lines: usize,
    ) {
        self.set_status_with_thinking_persona(
            header,
            details,
            details_capitalization,
            details_max_lines,
            ThinkingPersona::None,
        );
    }

    pub(super) fn set_status_with_thinking_persona(
        &mut self,
        header: String,
        details: Option<String>,
        details_capitalization: StatusDetailsCapitalization,
        details_max_lines: usize,
        thinking_persona: ThinkingPersona,
    ) {
        let details = details
            .filter(|details| !details.is_empty())
            .map(|details| {
                let trimmed = details.trim_start();
                match details_capitalization {
                    StatusDetailsCapitalization::CapitalizeFirst => {
                        crate::text_formatting::capitalize_first(trimmed)
                    }
                    StatusDetailsCapitalization::Preserve => trimmed.to_string(),
                }
            });
        self.current_status = StatusIndicatorState {
            header: header.clone(),
            details: details.clone(),
            details_max_lines,
            thinking_persona,
        };
        self.refresh_rendered_status_state();
    }

    /// Convenience wrapper around [`Self::set_status`];
    /// updates the status indicator header and clears any existing details.
    pub(super) fn set_status_header(&mut self, header: String) {
        self.set_status(
            header,
            /*details*/ None,
            StatusDetailsCapitalization::CapitalizeFirst,
            STATUS_DETAILS_DEFAULT_MAX_LINES,
        );
    }

    /// Sets the currently rendered footer status-line value.
    pub(crate) fn set_status_line(&mut self, status_line: Option<Line<'static>>) {
        self.bottom_pane.set_status_line(status_line);
    }

    /// Forwards the contextual active-agent label into the bottom-pane footer pipeline.
    ///
    /// `ChatWidget` stays a pass-through here so `App` remains the owner of "which thread is the
    /// user actually looking at?" and the footer stack remains a pure renderer of that decision.
    pub(crate) fn set_active_agent_label(&mut self, active_agent_label: Option<String>) {
        self.bottom_pane.set_active_agent_label(active_agent_label);
    }

    pub(super) fn refresh_rendered_status_state(&mut self) {
        self.turn_status_snapshot.set_base_status(
            self.current_status.header.clone(),
            self.current_status.details.clone(),
            self.current_status.details_max_lines,
        );
        let snapshot = self.turn_status_snapshot.status_snapshot();
        self.bottom_pane.update_status(
            snapshot.header,
            snapshot.details,
            StatusDetailsCapitalization::CapitalizeFirst,
            snapshot.details_max_lines,
        );
        self.bottom_pane
            .set_status_thinking_persona(self.current_status.thinking_persona);
        self.bottom_pane
            .set_status_activity_message(snapshot.activity_message);
        self.bottom_pane
            .set_status_footer_message(snapshot.footer_message);
        self.sync_work_panel_live_status();
        self.refresh_status_surfaces();
    }

    /// Recomputes footer status-line content from config and current runtime state.
    ///
    /// This method is the status-line orchestrator: it parses configured item identifiers,
    /// warns once per session about invalid items, updates whether status-line mode is enabled,
    /// schedules async git-branch lookup when needed, and renders only values that are currently
    /// available.
    ///
    /// The omission behavior is intentional. If selected items are unavailable (for example before
    /// a session id exists or before branch lookup completes), those items are skipped without
    /// placeholders so the line remains compact and stable.
    pub(crate) fn refresh_status_line(&mut self) {
        self.refresh_status_surfaces();
    }

    /// Records that status-line setup was canceled.
    ///
    /// Cancellation is intentionally side-effect free for config state; the existing configuration
    /// remains active and no persistence is attempted.
    pub(crate) fn cancel_status_line_setup(&self) {
        tracing::info!("Status line setup canceled by user");
    }

    /// Applies status-line item selection from the setup view to in-memory config.
    ///
    /// An empty selection persists as an explicit empty list.
    pub(crate) fn setup_status_line(&mut self, items: Vec<StatusLineItem>) {
        tracing::info!("status line setup confirmed with items: {items:#?}");
        let ids = items.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.tui_config.status_line = Some(ids);
        self.refresh_status_line();
    }

    /// Applies a temporary terminal-title selection while the setup UI is open.
    pub(crate) fn preview_terminal_title(&mut self, items: Vec<TerminalTitleItem>) {
        if self.terminal_title_setup_original_items.is_none() {
            self.terminal_title_setup_original_items = Some(self.tui_config.terminal_title.clone());
        }

        let ids = items.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.tui_config.terminal_title = Some(ids);
        self.refresh_terminal_title();
    }

    /// Restores the terminal-title config that was active before the setup UI
    /// opened, undoing any preview changes. No-op if no setup session is active.
    pub(crate) fn revert_terminal_title_setup_preview(&mut self) {
        let Some(original_items) = self.terminal_title_setup_original_items.take() else {
            return;
        };

        self.tui_config.terminal_title = original_items;
        self.refresh_terminal_title();
    }

    /// Dismisses the terminal-title setup UI and reverts to the pre-setup config.
    pub(crate) fn cancel_terminal_title_setup(&mut self) {
        tracing::info!("Terminal title setup canceled by user");
        self.revert_terminal_title_setup_preview();
    }

    /// Commits a confirmed terminal-title selection, ending the setup session.
    ///
    /// After this call, `revert_terminal_title_setup_preview` becomes a no-op
    /// because the original config snapshot is discarded.
    pub(crate) fn setup_terminal_title(&mut self, items: Vec<TerminalTitleItem>) {
        tracing::info!("terminal title setup confirmed with items: {items:#?}");
        let ids = items.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.terminal_title_setup_original_items = None;
        self.tui_config.terminal_title = Some(ids);
        self.refresh_terminal_title();
    }

    /// Stores async git-branch lookup results for the current status-line cwd.
    ///
    /// Results are dropped when they target an out-of-date cwd to avoid rendering stale branch
    /// names after directory changes.
    pub(crate) fn set_status_line_branch(&mut self, cwd: PathBuf, branch: Option<String>) {
        if self.status_line_branch_cwd.as_ref() != Some(&cwd) {
            self.status_line_branch_pending = false;
            return;
        }
        self.status_line_branch = branch;
        self.status_line_branch_pending = false;
        self.status_line_branch_lookup_complete = true;
        self.refresh_status_surfaces();
    }

    pub(super) fn collect_runtime_metrics_delta(&mut self) {
        if let Some(delta) = self.session_telemetry.runtime_metrics_summary() {
            self.apply_runtime_metrics_delta(delta);
        }
    }

    pub(super) fn apply_runtime_metrics_delta(&mut self, delta: RuntimeMetricsSummary) {
        let should_log_timing = has_websocket_timing_metrics(delta);
        self.turn_runtime_metrics.merge(delta);
        if should_log_timing {
            self.log_websocket_timing_totals(delta);
        }
    }

    pub(super) fn log_websocket_timing_totals(&mut self, delta: RuntimeMetricsSummary) {
        if let Some(label) = history_cell::runtime_metrics_label(delta.responses_api_summary()) {
            self.add_plain_history_lines(vec![
                vec!["• ".dim(), format!("WebSocket timing: {label}").dark_gray()].into(),
            ]);
        }
    }

    pub(super) fn refresh_runtime_metrics(&mut self) {
        self.collect_runtime_metrics_delta();
    }

    pub(super) fn restore_retry_status_header_if_present(&mut self) {
        if let Some(header) = self.retry_status_header.take() {
            self.set_status_header(header);
        }
    }
}

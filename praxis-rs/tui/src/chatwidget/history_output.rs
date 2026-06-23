use super::*;

impl ChatWidget {
    pub(crate) fn handle_history_entry_response(
        &mut self,
        event: praxis_protocol::protocol::GetHistoryEntryResponseEvent,
    ) {
        let praxis_protocol::protocol::GetHistoryEntryResponseEvent {
            offset,
            log_id,
            entry,
        } = event;
        self.bottom_pane
            .on_history_entry_response(log_id, offset, entry.map(|e| e.text));
    }

    pub(super) fn flush_active_cell(&mut self) {
        if let Some(active) = self.active_cell.take() {
            self.needs_final_message_separator = true;
            self.app_event_tx.send(AppEvent::InsertHistoryCell(active));
        }
    }

    pub(crate) fn add_to_history(&mut self, cell: impl HistoryCell + 'static) {
        self.add_boxed_history(Box::new(cell));
    }

    pub(super) fn add_boxed_history(&mut self, cell: Box<dyn HistoryCell>) {
        // Keep the placeholder session header as the active cell until real session info arrives,
        // so we can merge headers instead of committing a duplicate box to history.
        let keep_placeholder_header_active = !self.is_session_configured()
            && self
                .active_cell
                .as_ref()
                .is_some_and(|c| c.as_any().is::<history_cell::SessionHeaderHistoryCell>());

        if !keep_placeholder_header_active && !cell.display_lines(u16::MAX).is_empty() {
            // Only break exec grouping if the cell renders visible lines.
            self.flush_active_cell();
            self.needs_final_message_separator = true;
        }
        self.app_event_tx.send(AppEvent::InsertHistoryCell(cell));
    }

    /// Mark the active cell as failed (✗) and flush it into history.
    pub(super) fn finalize_active_cell_as_failed(&mut self) {
        if let Some(mut cell) = self.active_cell.take() {
            // Insert finalized cell into history and keep grouping consistent.
            if let Some(exec) = cell.as_any_mut().downcast_mut::<ExecCell>() {
                exec.mark_failed();
            } else if let Some(tool) = cell.as_any_mut().downcast_mut::<McpToolCallCell>() {
                tool.mark_failed();
            }
            self.add_boxed_history(cell);
        }
    }

    pub(crate) fn set_pending_thread_approvals(&mut self, threads: Vec<String>) {
        self.pending_thread_approvals_count = threads.len();
        self.bottom_pane.set_pending_thread_approvals(threads);
        self.sync_work_panel_queue();
    }

    pub(crate) fn add_diff_in_progress(&mut self) {
        self.request_redraw();
    }

    pub(crate) fn on_diff_complete(&mut self) {
        self.request_redraw();
    }

    pub(crate) fn add_debug_config_output(&mut self) {
        self.add_to_history(crate::debug_config::new_debug_config_output(
            &self.config,
            self.session_network_proxy.as_ref(),
        ));
    }

    pub(crate) fn add_ps_output(&mut self) {
        let processes = self
            .unified_exec_processes
            .iter()
            .map(|process| history_cell::UnifiedExecProcessDetails {
                command_display: process.command_display.clone(),
                recent_chunks: process.recent_chunks.clone(),
            })
            .collect();
        self.add_to_history(history_cell::new_unified_exec_processes_output(processes));
    }

    pub(super) fn clean_background_terminals(&mut self) {
        self.submit_op(AppCommand::clean_background_terminals());
        self.add_info_message(
            "Stopping all background terminals.".to_string(),
            /*hint*/ None,
        );
    }

    pub(crate) fn add_info_message(&mut self, message: String, hint: Option<String>) {
        self.add_to_history(history_cell::new_info_event(message, hint));
        self.request_redraw();
    }

    pub(crate) fn add_model_change_message(&mut self, message: String) {
        self.add_to_history(history_cell::new_model_change_divider(message));
        self.request_redraw();
    }

    pub(crate) fn add_plain_history_lines(&mut self, lines: Vec<Line<'static>>) {
        self.add_boxed_history(Box::new(PlainHistoryCell::new(lines)));
        self.request_redraw();
    }

    pub(crate) fn add_error_message(&mut self, message: String) {
        self.show_error_toast(truncate_text(&message, /*max_graphemes*/ 90));
        self.add_to_history(history_cell::new_error_event(message));
        self.request_redraw();
    }

    pub(super) fn add_app_gateway_stub_message(&mut self, feature: &str) {
        warn!(feature, "stubbed unsupported TUI feature");
        self.add_error_message(format!("{feature}: {TUI_STUB_MESSAGE}"));
    }

    pub(super) fn rename_confirmation_cell(
        name: &str,
        thread_id: Option<ThreadId>,
    ) -> PlainHistoryCell {
        let resume_cmd = praxis_core::util::resume_command(Some(name), thread_id)
            .unwrap_or_else(|| format!("{} resume {name}", praxis_core::util::PRIMARY_CLI_COMMAND));
        let name = name.to_string();
        let line = vec![
            "• ".into(),
            "Thread renamed to ".into(),
            name.cyan(),
            ", to resume this thread run ".into(),
            resume_cmd.cyan(),
        ];
        PlainHistoryCell::new(vec![line.into()])
    }

    /// Begin the asynchronous MCP inventory flow: show a loading spinner and
    /// request the app-gateway fetch via `AppEvent::FetchMcpInventory`.
    ///
    /// The spinner lives in `active_cell` and is cleared by
    /// [`clear_mcp_inventory_loading`] once the result arrives.
    pub(crate) fn add_mcp_output(&mut self) {
        self.flush_answer_stream_with_separator();
        self.flush_active_cell();
        self.active_cell = Some(Box::new(history_cell::new_mcp_inventory_loading(
            self.tui_config.animations,
        )));
        self.bump_active_cell_revision();
        self.request_redraw();
        self.app_event_tx.send(AppEvent::FetchMcpInventory);
    }

    /// Remove the MCP loading spinner if it is still the active cell.
    ///
    /// Uses `Any`-based type checking so that a late-arriving inventory result
    /// does not accidentally clear an unrelated cell that was set in the meantime.
    pub(crate) fn clear_mcp_inventory_loading(&mut self) {
        let Some(active) = self.active_cell.as_ref() else {
            return;
        };
        if !active
            .as_any()
            .is::<history_cell::McpInventoryLoadingCell>()
        {
            return;
        }
        self.active_cell = None;
        self.bump_active_cell_revision();
        self.request_redraw();
    }

    /// Forward file-search results to the bottom pane.
    pub(crate) fn apply_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.bottom_pane.on_file_search_result(query, matches);
    }
}

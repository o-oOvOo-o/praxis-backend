impl BottomPane {
    /// Show a generic list selection view with the provided items.
    pub(crate) fn show_selection_view(&mut self, params: list_selection_view::SelectionViewParams) {
        let view = list_selection_view::ListSelectionView::new(params, self.app_event_tx.clone());
        self.push_view(Box::new(view));
    }

    pub(crate) fn show_plugin_status_view(&mut self, document: PluginStatusDocument) {
        self.push_view(Box::new(PluginStatusView::new(document)));
    }

    pub(crate) fn replace_plugin_status_view_if_active(
        &mut self,
        document: PluginStatusDocument,
    ) -> bool {
        let is_match = self
            .view_stack
            .last()
            .is_some_and(|view| view.view_id() == Some(PLUGIN_STATUS_VIEW_ID));
        if !is_match {
            return false;
        }

        self.view_stack.pop();
        self.show_plugin_status_view(document);
        true
    }

    pub(crate) fn has_active_view(&self) -> bool {
        self.active_view().is_some()
    }

    pub(crate) fn active_view_fills_workspace(&self) -> bool {
        self.active_view().is_some_and(|view| {
            view.height_policy() == bottom_pane_view::BottomPaneViewHeight::FillWorkspace
        })
    }

    /// Replace the active selection view when it matches `view_id`.
    pub(crate) fn replace_selection_view_if_active(
        &mut self,
        view_id: &'static str,
        params: list_selection_view::SelectionViewParams,
    ) -> bool {
        let is_match = self
            .view_stack
            .last()
            .is_some_and(|view| view.view_id() == Some(view_id));
        if !is_match {
            return false;
        }

        self.view_stack.pop();
        let view = list_selection_view::ListSelectionView::new(params, self.app_event_tx.clone());
        self.push_view(Box::new(view));
        true
    }

    pub(crate) fn selected_index_for_active_view(&self, view_id: &'static str) -> Option<usize> {
        self.view_stack
            .last()
            .filter(|view| view.view_id() == Some(view_id))
            .and_then(|view| view.selected_index())
    }

    /// Update the pending-input preview shown above the composer.
    pub(crate) fn set_pending_input_preview(
        &mut self,
        queued: Vec<String>,
        pending_steers: Vec<String>,
        rejected_steers: Vec<String>,
    ) {
        self.pending_input_preview.pending_steers = pending_steers;
        self.pending_input_preview.rejected_steers = rejected_steers;
        self.pending_input_preview.queued_messages = queued;
        self.request_redraw();
    }

    /// Update the inactive-thread approval list shown above the composer.
    pub(crate) fn set_pending_thread_approvals(&mut self, threads: Vec<String>) {
        if self.pending_thread_approvals.set_threads(threads) {
            self.request_redraw();
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_thread_approvals(&self) -> &[String] {
        self.pending_thread_approvals.threads()
    }

    /// Update the unified-exec process set and refresh whichever summary surface is active.
    ///
    /// The summary may be displayed inline in the status row or as a dedicated
    /// footer row depending on whether a status indicator is currently visible.
    pub(crate) fn set_unified_exec_processes(&mut self, processes: Vec<String>) {
        if self.unified_exec_footer.set_processes(processes) {
            self.sync_status_inline_message();
            self.request_redraw();
        }
    }

    /// Copy unified-exec summary text into the active status row, if any.
    ///
    /// This keeps status-line inline text synchronized without forcing the
    /// standalone unified-exec footer row to be visible.
    fn sync_status_inline_message(&mut self) {
        if let Some(status) = self.status.as_mut() {
            status.update_inline_message(self.unified_exec_footer.summary_text());
            status.update_activity_message(self.status_activity_message.clone());
            status.update_footer_message(self.status_footer_message.clone());
        }
    }

    pub(crate) fn set_status_activity_message(&mut self, message: Option<String>) {
        let next = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
        if self.status_activity_message == next {
            return;
        }
        self.status_activity_message = next;
        self.sync_status_inline_message();
        self.request_redraw();
    }

    pub(crate) fn set_status_footer_message(&mut self, message: Option<String>) {
        let next = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
        if self.status_footer_message == next {
            return;
        }
        self.status_footer_message = next;
        self.sync_status_inline_message();
        self.request_redraw();
    }
}

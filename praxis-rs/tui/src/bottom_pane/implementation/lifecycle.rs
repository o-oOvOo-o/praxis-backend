impl BottomPane {
    /// Height (terminal rows) required by the current bottom pane.
    pub(crate) fn request_redraw(&self) {
        self.frame_requester.schedule_frame();
    }

    pub(crate) fn request_redraw_in(&self, dur: Duration) {
        self.frame_requester.schedule_frame_in(dur);
    }

    // --- History helpers ---

    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.composer.set_history_metadata(log_id, entry_count);
    }

    pub(crate) fn flush_paste_burst_if_due(&mut self) -> bool {
        // Give the active view the first chance to flush paste-burst state so
        // overlays that reuse the composer behave consistently.
        if let Some(view) = self.view_stack.last_mut()
            && view.flush_paste_burst_if_due()
        {
            return true;
        }
        self.composer.flush_paste_burst_if_due()
    }

    pub(crate) fn is_in_paste_burst(&self) -> bool {
        // A view can hold paste-burst state independently of the primary
        // composer, so check it first.
        self.view_stack
            .last()
            .is_some_and(|view| view.is_in_paste_burst())
            || self.composer.is_in_paste_burst()
    }

    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) {
        let updated = self
            .composer
            .on_history_entry_response(log_id, offset, entry);

        if updated {
            self.composer.sync_popups();
            self.request_redraw();
        }
    }

    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.composer.on_file_search_result(query, matches);
        self.request_redraw();
    }

    pub(crate) fn attach_image(&mut self, path: PathBuf) {
        if self.view_stack.is_empty() {
            self.composer.attach_image(path);
            self.request_redraw();
        }
    }

    #[cfg(test)]
    pub(crate) fn take_recent_submission_images(&mut self) -> Vec<PathBuf> {
        self.composer.take_recent_submission_images()
    }

    pub(crate) fn take_recent_submission_images_with_placeholders(
        &mut self,
    ) -> Vec<LocalImageAttachment> {
        self.composer
            .take_recent_submission_images_with_placeholders()
    }

    pub(crate) fn prepare_inline_args_submission(
        &mut self,
        record_history: bool,
    ) -> Option<(String, Vec<TextElement>)> {
        self.composer.prepare_inline_args_submission(record_history)
    }

    pub(crate) fn set_status_line(&mut self, status_line: Option<Line<'static>>) {
        if self.composer.set_status_line(status_line) {
            self.request_redraw();
        }
    }

    pub(crate) fn set_status_line_enabled(&mut self, enabled: bool) {
        if self.composer.set_status_line_enabled(enabled) {
            self.request_redraw();
        }
    }

    /// Updates the contextual footer label and requests a redraw only when it changed.
    ///
    /// This keeps the footer plumbing cheap during thread transitions where `App` may recompute
    /// the label several times while the visible thread settles.
    pub(crate) fn set_active_agent_label(&mut self, active_agent_label: Option<String>) {
        if self.composer.set_active_agent_label(active_agent_label) {
            self.request_redraw();
        }
    }

    pub(crate) fn set_footer_right_badge(&mut self, footer_right_badge: Option<Line<'static>>) {
        if self.composer.set_footer_right_badge(footer_right_badge) {
            self.request_redraw();
        }
    }
}

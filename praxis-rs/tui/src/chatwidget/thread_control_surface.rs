use super::*;

impl ChatWidget {
    pub(crate) fn thread_id(&self) -> Option<ThreadId> {
        self.thread_id
    }

    pub(crate) fn set_thread_control_state(&mut self, control_state: Option<&ThreadControlState>) {
        self.thread_control_state = control_state.cloned();
        self.sync_work_panel_control();
        if let Some(control_state) = control_state
            && control_state.read_only
        {
            let label = thread_control_display_label(control_state);
            self.bottom_pane.set_composer_input_enabled(
                /*enabled*/ true,
                Some(format!(
                    "Locked by {label}; type /release-thread to take over"
                )),
            );
            return;
        }
        self.bottom_pane
            .set_composer_input_enabled(/*enabled*/ true, /*placeholder*/ None);
    }

    pub(super) fn reject_read_only_thread_submission(
        &mut self,
        text: &str,
        text_elements: &[TextElement],
    ) -> bool {
        let Some(label) = self.read_only_thread_control_label() else {
            return false;
        };
        self.bottom_pane
            .set_composer_text(text.to_string(), text_elements.to_vec(), Vec::new());
        self.add_info_message(
            format!("This thread is locked by {label}."),
            Some("Type /release-thread to take over before sending a message.".to_string()),
        );
        true
    }

    pub(super) fn read_only_thread_control_label(&self) -> Option<String> {
        self.thread_control_state
            .as_ref()
            .filter(|control_state| control_state.read_only)
            .map(thread_control_display_label)
    }

    pub(crate) fn thread_name(&self) -> Option<String> {
        self.thread_name.clone()
    }

    /// Returns the current thread's precomputed rollout path.
    ///
    /// For fresh non-ephemeral threads this path may exist before the file is
    /// materialized; rollout persistence is deferred until the first user
    /// message is recorded.
    pub(crate) fn rollout_path(&self) -> Option<PathBuf> {
        self.current_rollout_path.clone()
    }

    pub(crate) fn active_cell_mouse_action(
        &self,
        area: Rect,
        column: u16,
        row: u16,
    ) -> Option<history_cell::HistoryCellMouseAction> {
        let layout = self.layout_for_area(area);
        let content_area = layout.active_content_area?;
        if content_area.is_empty()
            || column < content_area.x
            || column >= content_area.right()
            || row < content_area.y
            || row >= content_area.bottom()
        {
            return None;
        }

        let cache = self.active_cell_render_cache(content_area.width)?;
        let scroll_offset = cache.desired_height.saturating_sub(content_area.height);
        let row_in_cell = row
            .saturating_sub(content_area.y)
            .saturating_add(scroll_offset);

        cache
            .mouse_targets
            .iter()
            .find(|target| target.contains_row(row_in_cell))
            .map(|target| target.action.clone())
    }

    /// Return a reference to the widget's current config (includes any
    /// runtime overrides applied via TUI, e.g., model or approval policy).
    pub(crate) fn config_ref(&self) -> &Config {
        &self.config
    }

    #[cfg(test)]
    pub(crate) fn tui_config_ref(&self) -> &TuiRuntimeConfig {
        &self.tui_config
    }

    pub(crate) fn set_tui_config(&mut self, tui_config: TuiRuntimeConfig) {
        self.tui_config = tui_config;
        self.bottom_pane
            .set_animations_enabled(self.tui_config.animations);
        self.sync_surface_theme();
        self.refresh_status_surfaces();
    }

    #[cfg(test)]
    pub(crate) fn status_line_text(&self) -> Option<String> {
        self.bottom_pane.status_line_text()
    }
}

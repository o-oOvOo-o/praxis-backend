impl BottomPane {
    /// Forward a key event to the active view or the composer.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        // If a modal/view is active, handle it here; otherwise forward to composer.
        if !self.view_stack.is_empty() {
            if key_event.kind == KeyEventKind::Release {
                return InputResult::None;
            }

            // We need three pieces of information after routing the key:
            // whether Esc completed the view, whether the view finished for any
            // reason, and whether a paste-burst timer should be scheduled.
            let (ctrl_c_completed, view_complete, view_in_paste_burst) = {
                let last_index = self.view_stack.len() - 1;
                let view = &mut self.view_stack[last_index];
                let prefer_esc =
                    key_event.code == KeyCode::Esc && view.prefer_esc_to_handle_key_event();
                let ctrl_c_completed = key_event.code == KeyCode::Esc
                    && !prefer_esc
                    && matches!(view.on_ctrl_c(), CancellationEvent::Handled)
                    && view.is_complete();
                if ctrl_c_completed {
                    (true, true, false)
                } else {
                    view.handle_key_event(key_event);
                    (false, view.is_complete(), view.is_in_paste_burst())
                }
            };

            if ctrl_c_completed {
                self.view_stack.pop();
                self.on_active_view_complete();
                if let Some(next_view) = self.view_stack.last()
                    && next_view.is_in_paste_burst()
                {
                    self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
                }
            } else if view_complete {
                self.view_stack.clear();
                self.on_active_view_complete();
            } else if view_in_paste_burst {
                self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
            }
            self.request_redraw();
            InputResult::None
        } else {
            let is_agent_command = self
                .composer_text()
                .lines()
                .next()
                .and_then(parse_slash_name)
                .is_some_and(|(name, _, _)| name == "agent");

            // If a task is running and a status line is visible, allow Esc to
            // send an interrupt even while the composer has focus.
            // When a popup is active, prefer dismissing it over interrupting the task.
            if key_event.code == KeyCode::Esc
                && matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
                && self.is_task_running
                && !is_agent_command
                && !self.composer.popup_active()
                && let Some(status) = &self.status
            {
                // Send Op::Interrupt
                status.interrupt();
                self.request_redraw();
                return InputResult::None;
            }
            let (input_result, needs_redraw) = self.composer.handle_key_event(key_event);
            if needs_redraw {
                self.request_redraw();
            }
            if self.composer.is_in_paste_burst() {
                self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
            }
            input_result
        }
    }

    pub fn handle_mouse_event(&mut self, mouse_event: &MouseEvent) -> bool {
        if self.view_stack.is_empty() {
            return false;
        }

        let (handled, view_complete, view_in_paste_burst) = {
            let last_index = self.view_stack.len() - 1;
            let view = &mut self.view_stack[last_index];
            let handled = view.handle_mouse_event(mouse_event);
            (handled, view.is_complete(), view.is_in_paste_burst())
        };

        if view_complete {
            self.view_stack.clear();
            self.on_active_view_complete();
        } else if view_in_paste_burst {
            self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
        }
        if handled || view_complete || view_in_paste_burst {
            self.request_redraw();
        }
        handled || view_complete
    }

    /// Handles a Ctrl+C press within the bottom pane.
    ///
    /// An active modal view is given the first chance to consume the key (typically to dismiss
    /// itself). If no view is active, Ctrl+C clears draft composer input.
    ///
    /// This method does not decide whether the process should exit; `ChatWidget` owns the
    /// quit/interrupt state machine and uses the result to decide what happens next.
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        if let Some(view) = self.view_stack.last_mut() {
            let event = view.on_ctrl_c();
            if matches!(event, CancellationEvent::Handled) {
                if view.is_complete() {
                    self.view_stack.pop();
                    self.on_active_view_complete();
                }
                self.request_redraw();
            }
            event
        } else if self.composer_is_empty() {
            CancellationEvent::NotHandled
        } else {
            self.view_stack.pop();
            self.clear_composer_for_ctrl_c();
            self.request_redraw();
            CancellationEvent::Handled
        }
    }

    pub fn handle_paste(&mut self, pasted: String) {
        if !self.view_stack.is_empty() {
            let (needs_redraw, view_complete) = {
                let last_index = self.view_stack.len() - 1;
                let view = &mut self.view_stack[last_index];
                (view.handle_paste(pasted), view.is_complete())
            };
            if view_complete {
                self.view_stack.clear();
                self.on_active_view_complete();
            }
            if needs_redraw {
                self.request_redraw();
            }
        } else {
            let needs_redraw = self.composer.handle_paste(pasted);
            self.composer.sync_popups();
            if needs_redraw {
                self.request_redraw();
            }
        }
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.composer.insert_str(text);
        self.composer.sync_popups();
        self.request_redraw();
    }

    pub(crate) fn pre_draw_tick(&mut self, terminal_focused: bool) {
        self.composer.sync_popups();
        if let Some(status) = self.status.as_mut() {
            status.set_terminal_focused(terminal_focused);
        }
    }

    /// Replace the composer text with `text`.
    ///
    /// This is intended for fresh input where mention linkage does not need to
    /// survive; it routes to `ChatComposer::set_text_content`, which resets
    /// mention bindings.
    pub(crate) fn set_composer_text(
        &mut self,
        text: String,
        text_elements: Vec<TextElement>,
        local_image_paths: Vec<PathBuf>,
    ) {
        self.composer
            .set_text_content(text, text_elements, local_image_paths);
        self.composer.move_cursor_to_end();
        self.request_redraw();
    }

    /// Replace the composer text while preserving mention link targets.
    ///
    /// Use this when rehydrating a draft after a local validation/gating
    /// failure (for example unsupported image submit) so previously selected
    /// mention targets remain stable across retry.
    pub(crate) fn set_composer_text_with_mention_bindings(
        &mut self,
        text: String,
        text_elements: Vec<TextElement>,
        local_image_paths: Vec<PathBuf>,
        mention_bindings: Vec<MentionBinding>,
    ) {
        self.composer.set_text_content_with_mention_bindings(
            text,
            text_elements,
            local_image_paths,
            mention_bindings,
        );
        self.request_redraw();
    }

    #[allow(dead_code)]
    pub(crate) fn set_composer_input_enabled(
        &mut self,
        enabled: bool,
        placeholder: Option<String>,
    ) {
        self.composer.set_input_enabled(enabled, placeholder);
        self.request_redraw();
    }

    pub(crate) fn clear_composer_for_ctrl_c(&mut self) {
        self.composer.clear_for_ctrl_c();
        self.request_redraw();
    }

    /// Get the current composer text (for tests and programmatic checks).
    pub(crate) fn composer_text(&self) -> String {
        self.composer.current_text()
    }

    pub(crate) fn composer_text_elements(&self) -> Vec<TextElement> {
        self.composer.text_elements()
    }

    pub(crate) fn composer_local_images(&self) -> Vec<LocalImageAttachment> {
        self.composer.local_images()
    }

    pub(crate) fn composer_mention_bindings(&self) -> Vec<MentionBinding> {
        self.composer.mention_bindings()
    }

    #[cfg(test)]
    pub(crate) fn composer_local_image_paths(&self) -> Vec<PathBuf> {
        self.composer.local_image_paths()
    }

    pub(crate) fn composer_text_with_pending(&self) -> String {
        self.composer.current_text_with_pending()
    }

    pub(crate) fn composer_pending_pastes(&self) -> Vec<(String, String)> {
        self.composer.pending_pastes()
    }

    pub(crate) fn apply_external_edit(&mut self, text: String) {
        self.composer.apply_external_edit(text);
        self.request_redraw();
    }

    pub(crate) fn set_footer_hint_override(&mut self, items: Option<Vec<(String, String)>>) {
        self.composer.set_footer_hint_override(items);
        self.request_redraw();
    }

    pub(crate) fn set_remote_image_urls(&mut self, urls: Vec<String>) {
        self.composer.set_remote_image_urls(urls);
        self.request_redraw();
    }

    pub(crate) fn remote_image_urls(&self) -> Vec<String> {
        self.composer.remote_image_urls()
    }

    pub(crate) fn take_remote_image_urls(&mut self) -> Vec<String> {
        let urls = self.composer.take_remote_image_urls();
        self.request_redraw();
        urls
    }

    pub(crate) fn set_composer_pending_pastes(&mut self, pending_pastes: Vec<(String, String)>) {
        self.composer.set_pending_pastes(pending_pastes);
        self.request_redraw();
    }
}

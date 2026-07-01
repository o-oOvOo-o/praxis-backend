use super::*;

impl ChatWidget {
    pub(crate) fn ui_language(&self) -> UiLanguage {
        self.ui_language
    }

    pub(crate) fn set_ui_language(&mut self, language: UiLanguage) {
        self.ui_language = language;
        self.request_redraw();
    }

    pub(super) fn handle_language_command(&mut self, args: &str) {
        let trimmed = args.trim();
        let next_language = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("toggle") {
            Some(self.ui_language.toggled())
        } else if trimmed.eq_ignore_ascii_case("status") {
            self.add_info_message(self.ui_language.language_status_message(), None);
            self.bottom_pane.drain_pending_submission_state();
            return;
        } else {
            UiLanguage::parse(trimmed)
        };

        match next_language {
            Some(language) => {
                self.set_ui_language(language);
                self.add_info_message(language.language_changed_message(), None);
                self.show_info_toast(format!("Language: {}", language.display_name()));
            }
            None if trimmed.is_empty() => {
                self.add_info_message(self.ui_language.language_usage_message().to_string(), None);
            }
            None => {
                self.add_error_message(self.ui_language.invalid_language_message(trimmed));
            }
        }
        self.bottom_pane.drain_pending_submission_state();
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                kind: KeyEventKind::Press,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) && c.eq_ignore_ascii_case(&'c') => {
                self.on_ctrl_c();
                return;
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                kind: KeyEventKind::Press,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) && c.eq_ignore_ascii_case(&'d') => {
                if self.on_ctrl_d() {
                    return;
                }
                self.bottom_pane.clear_quit_shortcut_hint();
                self.quit_shortcut_expires_at = None;
                self.quit_shortcut_key = None;
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                kind: KeyEventKind::Press,
                ..
            } if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && c.eq_ignore_ascii_case(&'v') =>
            {
                match paste_image_to_temp_png() {
                    Ok((path, info)) => {
                        tracing::debug!(
                            "pasted image size={}x{} format={}",
                            info.width,
                            info.height,
                            info.encoded_format.label()
                        );
                        self.attach_image(path);
                    }
                    Err(err) => {
                        tracing::warn!("failed to paste image: {err}");
                        self.add_to_history(history_cell::new_error_event(format!(
                            "Failed to paste image: {err}",
                        )));
                    }
                }
                return;
            }
            other if other.kind == KeyEventKind::Press => {
                self.bottom_pane.clear_quit_shortcut_hint();
                self.quit_shortcut_expires_at = None;
                self.quit_shortcut_key = None;
            }
            _ => {}
        }

        if key_event.kind == KeyEventKind::Press
            && self.queued_message_edit_binding.is_press(key_event)
            && self.has_queued_follow_up_messages()
        {
            if let Some(user_message) = self.pop_latest_queued_user_message() {
                self.restore_user_message_to_composer(user_message);
                self.refresh_pending_input_preview();
                self.request_redraw();
            }
            return;
        }

        if matches!(key_event.code, KeyCode::Esc)
            && matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            && !self.pending_steers.is_empty()
            && self.bottom_pane.is_task_running()
            && self.bottom_pane.no_modal_or_popup_active()
        {
            if self.request_pending_steer_interrupt() && !self.submit_op(AppCommand::interrupt()) {
                self.rollback_pending_steer_interrupt();
            }
            return;
        }

        match key_event {
            KeyEvent {
                code: KeyCode::BackTab,
                kind: KeyEventKind::Press,
                ..
            } if self.collaboration_modes_enabled()
                && !self.bottom_pane.is_task_running()
                && self.bottom_pane.no_modal_or_popup_active() =>
            {
                self.cycle_collaboration_mode();
            }
            _ => match self.bottom_pane.handle_key_event(key_event) {
                InputResult::Submitted {
                    text,
                    text_elements,
                } => {
                    if self.reject_read_only_thread_submission(&text, &text_elements) {
                        return;
                    }
                    let local_images = self
                        .bottom_pane
                        .take_recent_submission_images_with_placeholders();
                    let remote_image_urls = self.take_remote_image_urls();
                    let user_message = UserMessage {
                        text,
                        local_images,
                        remote_image_urls,
                        text_elements,
                        mention_bindings: self
                            .bottom_pane
                            .take_recent_submission_mention_bindings(),
                    };
                    if user_message.text.is_empty()
                        && user_message.local_images.is_empty()
                        && user_message.remote_image_urls.is_empty()
                    {
                        return;
                    }
                    let Some(user_message) =
                        self.maybe_defer_user_message_for_realtime(user_message)
                    else {
                        return;
                    };
                    let should_submit_now =
                        self.is_session_configured() && !self.is_plan_streaming_in_tui();
                    if should_submit_now {
                        // Submitted is emitted when user submits.
                        // Reset any reasoning header only when we are actually submitting a turn.
                        self.reasoning_buffer.clear();
                        self.full_reasoning_buffer.clear();
                        self.reasoning_block_kind = None;
                        self.set_status_header(GENERIC_STATUS_HEADER.to_string());
                        self.submit_user_message(user_message);
                    } else {
                        self.queue_user_message(user_message);
                    }
                }
                InputResult::Queued {
                    text,
                    text_elements,
                } => {
                    if self.reject_read_only_thread_submission(&text, &text_elements) {
                        return;
                    }
                    let local_images = self
                        .bottom_pane
                        .take_recent_submission_images_with_placeholders();
                    let remote_image_urls = self.take_remote_image_urls();
                    let user_message = UserMessage {
                        text,
                        local_images,
                        remote_image_urls,
                        text_elements,
                        mention_bindings: self
                            .bottom_pane
                            .take_recent_submission_mention_bindings(),
                    };
                    let Some(user_message) =
                        self.maybe_defer_user_message_for_realtime(user_message)
                    else {
                        return;
                    };
                    self.queue_user_message(user_message);
                }
                InputResult::Command(cmd) => {
                    self.dispatch_command(cmd);
                }
                InputResult::CommandWithArgs(cmd, args, text_elements) => {
                    self.dispatch_command_with_args(cmd, args, text_elements);
                }
                InputResult::ThreadCommand(intent) => {
                    self.dispatch_external_thread_command(intent);
                }
                InputResult::None => {}
            },
        }
    }

    /// Attach a local image to the composer when the active model supports image inputs.
    ///
    /// When the model does not advertise image support, we keep the draft unchanged and surface a
    /// warning event so users can switch models or remove attachments.
    pub(crate) fn attach_image(&mut self, path: PathBuf) {
        if !self.current_model_supports_images() {
            self.add_to_history(history_cell::new_warning_event(
                self.image_inputs_not_supported_message(),
            ));
            self.request_redraw();
            return;
        }
        tracing::info!("attach_image path={path:?}");
        self.bottom_pane.attach_image(path);
        self.request_redraw();
    }

    pub(crate) fn composer_text_with_pending(&self) -> String {
        self.bottom_pane.composer_text_with_pending()
    }

    pub(crate) fn apply_external_edit(&mut self, text: String) {
        self.bottom_pane.apply_external_edit(text);
        self.request_redraw();
    }

    pub(crate) fn external_editor_state(&self) -> ExternalEditorState {
        self.external_editor_state
    }

    pub(crate) fn set_external_editor_state(&mut self, state: ExternalEditorState) {
        self.external_editor_state = state;
    }

    pub(crate) fn set_footer_hint_override(&mut self, items: Option<Vec<(String, String)>>) {
        self.bottom_pane.set_footer_hint_override(items);
    }

    pub(crate) fn show_selection_view(&mut self, params: SelectionViewParams) {
        self.bottom_pane.show_selection_view(params);
        self.request_redraw();
    }

    pub(crate) fn no_modal_or_popup_active(&self) -> bool {
        self.bottom_pane.no_modal_or_popup_active()
    }

    pub(crate) fn can_launch_external_editor(&self) -> bool {
        self.bottom_pane.can_launch_external_editor()
    }

    pub(crate) fn can_run_ctrl_l_clear_now(&mut self) -> bool {
        // Ctrl+L is not a slash command, but it follows /clear's current rule:
        // block while a task is running.
        if !self.bottom_pane.is_task_running() {
            return true;
        }

        let message = "Ctrl+L is disabled while a task is in progress.".to_string();
        self.add_to_history(history_cell::new_error_event(message));
        self.request_redraw();
        false
    }

    pub(super) fn show_rename_prompt(&mut self) {
        let tx = self.app_event_tx.clone();
        let has_name = self
            .thread_name
            .as_ref()
            .is_some_and(|name| !name.is_empty());
        let title = if has_name {
            "Rename thread"
        } else {
            "Name thread"
        };
        let thread_id = self.thread_id;
        let view = CustomPromptView::new(
            title.to_string(),
            "Type a name and press Enter".to_string(),
            /*context_label*/ None,
            Box::new(move |name: String| {
                let Some(name) = praxis_core::util::normalize_thread_name(&name) else {
                    tx.send(AppEvent::InsertHistoryCell(Box::new(
                        history_cell::new_error_event("Thread name cannot be empty.".to_string()),
                    )));
                    return;
                };
                let cell = Self::rename_confirmation_cell(&name, thread_id);
                tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
                tx.set_thread_name(name);
            }),
        );

        self.bottom_pane.show_view(Box::new(view));
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        self.bottom_pane.handle_paste(text);
    }

    // Returns true if caller should skip rendering this frame (a future frame is scheduled).
    pub(crate) fn handle_paste_burst_tick(&mut self, frame_requester: FrameRequester) -> bool {
        if self.bottom_pane.flush_paste_burst_if_due() {
            // A paste just flushed; request an immediate redraw and skip this frame.
            self.request_redraw();
            true
        } else if self.bottom_pane.is_in_paste_burst() {
            // While capturing a burst, schedule a follow-up tick and skip this frame
            // to avoid redundant renders between ticks.
            frame_requester.schedule_frame_in(
                crate::bottom_pane::ChatComposer::recommended_paste_flush_delay(),
            );
            true
        } else {
            false
        }
    }
}

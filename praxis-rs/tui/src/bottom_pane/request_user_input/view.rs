use super::*;

impl BottomPaneView for RequestUserInputOverlay {
    fn prefer_esc_to_handle_key_event(&self) -> bool {
        true
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Release {
            return;
        }

        if self.confirm_unanswered_active() {
            self.handle_confirm_unanswered_key_event(key_event);
            return;
        }

        if matches!(key_event.code, KeyCode::Esc) {
            if self.has_options() && self.notes_ui_visible() {
                self.clear_notes_and_focus_options();
                return;
            }
            // TODO: Emit interrupted request_user_input results (including committed answers)
            // once core supports persisting them reliably without follow-up turn issues.
            self.app_event_tx.interrupt();
            self.done = true;
            return;
        }

        // Question navigation is always available.
        match key_event {
            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_question(/*next*/ false);
                return;
            }
            KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.move_question(/*next*/ true);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.has_options() && matches!(self.focus, Focus::Options) => {
                self.move_question(/*next*/ false);
                return;
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.has_options() && matches!(self.focus, Focus::Options) => {
                self.move_question(/*next*/ false);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.has_options() && matches!(self.focus, Focus::Options) => {
                self.move_question(/*next*/ true);
                return;
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.has_options() && matches!(self.focus, Focus::Options) => {
                self.move_question(/*next*/ true);
                return;
            }
            _ => {}
        }

        match self.focus {
            Focus::Options => {
                let options_len = self.options_len();
                // Keep selection synchronized as the user moves.
                match key_event.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        let moved = if let Some(answer) = self.current_answer_mut() {
                            answer.options_state.move_up_wrap(options_len);
                            answer.answer_committed = false;
                            true
                        } else {
                            false
                        };
                        if moved {
                            self.sync_composer_placeholder();
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let moved = if let Some(answer) = self.current_answer_mut() {
                            answer.options_state.move_down_wrap(options_len);
                            answer.answer_committed = false;
                            true
                        } else {
                            false
                        };
                        if moved {
                            self.sync_composer_placeholder();
                        }
                    }
                    KeyCode::Char(' ') => {
                        self.select_current_option(/*committed*/ true);
                    }
                    KeyCode::Backspace | KeyCode::Delete => {
                        self.clear_selection();
                    }
                    KeyCode::Tab => {
                        if self.selected_option_index().is_some() {
                            self.focus = Focus::Notes;
                            self.ensure_selected_for_notes();
                        }
                    }
                    KeyCode::Enter => {
                        let has_selection = self.selected_option_index().is_some();
                        if has_selection {
                            self.select_current_option(/*committed*/ true);
                        }
                        self.go_next_or_submit();
                    }
                    KeyCode::Char(ch) => {
                        if let Some(option_idx) = self.option_index_for_digit(ch) {
                            if let Some(answer) = self.current_answer_mut() {
                                answer.options_state.selected_idx = Some(option_idx);
                            }
                            self.select_current_option(/*committed*/ true);
                            self.go_next_or_submit();
                        }
                    }
                    _ => {}
                }
            }
            Focus::Notes => {
                let notes_empty = self.composer.current_text_with_pending().trim().is_empty();
                if self.has_options() && matches!(key_event.code, KeyCode::Tab) {
                    self.clear_notes_and_focus_options();
                    return;
                }
                if self.has_options() && matches!(key_event.code, KeyCode::Backspace) && notes_empty
                {
                    self.save_current_draft();
                    if let Some(answer) = self.current_answer_mut() {
                        answer.notes_visible = false;
                    }
                    self.focus = Focus::Options;
                    self.sync_composer_placeholder();
                    return;
                }
                if matches!(key_event.code, KeyCode::Enter) {
                    self.ensure_selected_for_notes();
                    self.pending_submission_draft = Some(self.capture_composer_draft());
                    let (result, _) = self.composer.handle_key_event(key_event);
                    if !self.handle_composer_input_result(result) {
                        self.pending_submission_draft = None;
                        if self.has_options() {
                            self.select_current_option(/*committed*/ true);
                        }
                        self.go_next_or_submit();
                    }
                    return;
                }
                if self.has_options() && matches!(key_event.code, KeyCode::Up | KeyCode::Down) {
                    let options_len = self.options_len();
                    match key_event.code {
                        KeyCode::Up => {
                            let moved = if let Some(answer) = self.current_answer_mut() {
                                answer.options_state.move_up_wrap(options_len);
                                answer.answer_committed = false;
                                true
                            } else {
                                false
                            };
                            if moved {
                                self.sync_composer_placeholder();
                            }
                        }
                        KeyCode::Down => {
                            let moved = if let Some(answer) = self.current_answer_mut() {
                                answer.options_state.move_down_wrap(options_len);
                                answer.answer_committed = false;
                                true
                            } else {
                                false
                            };
                            if moved {
                                self.sync_composer_placeholder();
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                self.ensure_selected_for_notes();
                if matches!(
                    key_event.code,
                    KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
                ) && let Some(answer) = self.current_answer_mut()
                {
                    answer.answer_committed = false;
                }
                let before = self.capture_composer_draft();
                let (result, _) = self.composer.handle_key_event(key_event);
                let submitted = self.handle_composer_input_result(result);
                if !submitted {
                    let after = self.capture_composer_draft();
                    if before != after
                        && let Some(answer) = self.current_answer_mut()
                    {
                        answer.answer_committed = false;
                    }
                }
            }
        }
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        if self.confirm_unanswered_active() {
            self.close_unanswered_confirmation();
            // TODO: Emit interrupted request_user_input results (including committed answers)
            // once core supports persisting them reliably without follow-up turn issues.
            self.app_event_tx.interrupt();
            self.done = true;
            return CancellationEvent::Handled;
        }
        if self.focus_is_notes() && !self.composer.current_text_with_pending().is_empty() {
            self.clear_notes_draft();
            return CancellationEvent::Handled;
        }

        // TODO: Emit interrupted request_user_input results (including committed answers)
        // once core supports persisting them reliably without follow-up turn issues.
        self.app_event_tx.interrupt();
        self.done = true;
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.done
    }

    fn handle_paste(&mut self, pasted: String) -> bool {
        if pasted.is_empty() {
            return false;
        }
        if matches!(self.focus, Focus::Options) {
            // Treat pastes the same as typing: switch into notes.
            self.focus = Focus::Notes;
        }
        self.ensure_selected_for_notes();
        if let Some(answer) = self.current_answer_mut() {
            answer.answer_committed = false;
        }
        self.composer.handle_paste(pasted)
    }

    fn flush_paste_burst_if_due(&mut self) -> bool {
        self.composer.flush_paste_burst_if_due()
    }

    fn is_in_paste_burst(&self) -> bool {
        self.composer.is_in_paste_burst()
    }

    fn try_consume_user_input_request(
        &mut self,
        request: RequestUserInputEvent,
    ) -> Option<RequestUserInputEvent> {
        self.queue.push_back(request);
        None
    }
}

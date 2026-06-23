use super::*;

impl BottomPaneView for McpServerElicitationOverlay {
    fn prefer_esc_to_handle_key_event(&self) -> bool {
        true
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Release {
            return;
        }

        if matches!(key_event.code, KeyCode::Esc) {
            self.dispatch_cancel();
            self.done = true;
            return;
        }

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
                self.move_field(/*next*/ false);
                return;
            }
            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_field(/*next*/ true);
                return;
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.current_field_is_select() => {
                self.move_field(/*next*/ false);
                return;
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.current_field_is_select() => {
                self.move_field(/*next*/ true);
                return;
            }
            _ => {}
        }

        if self.current_field_is_select() {
            self.validation_error = None;
            let options_len = self.options_len();
            match key_event.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Some(answer) = self.current_answer_mut() {
                        answer.selection.move_up_wrap(options_len);
                        answer.answer_committed = false;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Some(answer) = self.current_answer_mut() {
                        answer.selection.move_down_wrap(options_len);
                        answer.answer_committed = false;
                    }
                }
                KeyCode::Backspace | KeyCode::Delete => self.clear_selection(),
                KeyCode::Char(' ') => self.select_current_option(/*committed*/ true),
                KeyCode::Enter => {
                    if self.selected_option_index().is_some() {
                        self.select_current_option(/*committed*/ true);
                    }
                    self.go_next_or_submit();
                }
                KeyCode::Char(ch) => {
                    if let Some(option_idx) = self.option_index_for_digit(ch) {
                        if let Some(answer) = self.current_answer_mut() {
                            answer.selection.selected_idx = Some(option_idx);
                        }
                        self.select_current_option(/*committed*/ true);
                        self.go_next_or_submit();
                    }
                }
                _ => {}
            }
            return;
        }

        let before = self.capture_composer_draft();
        let (result, _) = self.composer.handle_key_event(key_event);
        let submitted = self.handle_composer_input_result(result);
        if submitted {
            return;
        }
        let after = self.capture_composer_draft();
        if before != after {
            self.validation_error = None;
            if let Some(answer) = self.current_answer_mut() {
                answer.answer_committed = false;
            }
        }
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        if !self.current_field_is_select() && !self.composer.current_text_with_pending().is_empty()
        {
            self.clear_current_draft();
            return CancellationEvent::Handled;
        }
        self.dispatch_cancel();
        self.done = true;
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.done
    }

    fn handle_paste(&mut self, pasted: String) -> bool {
        if pasted.is_empty() || self.current_field_is_select() {
            return false;
        }
        self.validation_error = None;
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

    fn try_consume_mcp_server_elicitation_request(
        &mut self,
        request: McpServerElicitationFormRequest,
    ) -> Option<McpServerElicitationFormRequest> {
        self.queue.push_back(request);
        None
    }
}

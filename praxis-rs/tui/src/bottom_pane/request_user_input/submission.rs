use super::*;

impl RequestUserInputOverlay {
    /// Advance to next question, or submit when on the last one.
    pub(super) fn go_next_or_submit(&mut self) {
        if self.current_index() + 1 >= self.question_count() {
            self.save_current_draft();
            if self.unanswered_count() > 0 {
                self.open_unanswered_confirmation();
            } else {
                self.submit_answers();
            }
        } else {
            self.move_question(/*next*/ true);
        }
    }

    /// Build the response payload and dispatch it to the app.
    pub(super) fn submit_answers(&mut self) {
        self.confirm_unanswered = None;
        self.save_current_draft();
        let mut answers = HashMap::new();
        for (idx, question) in self.request.questions.iter().enumerate() {
            let answer_state = &self.answers[idx];
            let options = question.options.as_ref();
            // For option questions we may still produce no selection.
            let selected_idx =
                if options.is_some_and(|opts| !opts.is_empty()) && answer_state.answer_committed {
                    answer_state.options_state.selected_idx
                } else {
                    None
                };
            // Notes are appended as extra answers. For freeform questions, only submit when
            // the user explicitly committed the draft.
            let notes = if answer_state.answer_committed {
                answer_state.draft.text_with_pending().trim().to_string()
            } else {
                String::new()
            };
            let selected_label = selected_idx
                .and_then(|selected_idx| Self::option_label_for_index(question, selected_idx));
            let mut answer_list = selected_label.into_iter().collect::<Vec<_>>();
            if !notes.is_empty() {
                answer_list.push(format!("user_note: {notes}"));
            }
            answers.insert(
                question.id.clone(),
                RequestUserInputAnswer {
                    answers: answer_list,
                },
            );
        }
        self.app_event_tx.user_input_answer(
            self.request.turn_id.clone(),
            RequestUserInputResponse {
                answers: answers.clone(),
            },
        );
        self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
            history_cell::RequestUserInputResultCell {
                questions: self.request.questions.clone(),
                answers,
                interrupted: false,
            },
        )));
        if let Some(next) = self.queue.pop_front() {
            self.request = next;
            self.reset_for_request();
            self.ensure_focus_available();
            self.restore_current_draft();
        } else {
            self.done = true;
        }
    }

    pub(super) fn open_unanswered_confirmation(&mut self) {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        self.confirm_unanswered = Some(state);
    }

    pub(super) fn close_unanswered_confirmation(&mut self) {
        self.confirm_unanswered = None;
    }

    pub(super) fn unanswered_question_count(&self) -> usize {
        self.unanswered_count()
    }

    pub(super) fn unanswered_submit_description(&self) -> String {
        let count = self.unanswered_question_count();
        let suffix = if count == 1 {
            UNANSWERED_CONFIRM_SUBMIT_DESC_SINGULAR
        } else {
            UNANSWERED_CONFIRM_SUBMIT_DESC_PLURAL
        };
        format!("Submit with {count} unanswered {suffix}.")
    }

    pub(super) fn first_unanswered_index(&self) -> Option<usize> {
        let current_text = self.composer.current_text();
        self.request
            .questions
            .iter()
            .enumerate()
            .find(|(idx, _)| !self.is_question_answered(*idx, &current_text))
            .map(|(idx, _)| idx)
    }

    pub(super) fn unanswered_confirmation_rows(&self) -> Vec<GenericDisplayRow> {
        let selected = self
            .confirm_unanswered
            .as_ref()
            .and_then(|state| state.selected_idx)
            .unwrap_or(0);
        let entries = [
            (
                UNANSWERED_CONFIRM_SUBMIT,
                self.unanswered_submit_description(),
            ),
            (
                UNANSWERED_CONFIRM_GO_BACK,
                UNANSWERED_CONFIRM_GO_BACK_DESC.to_string(),
            ),
        ];
        entries
            .iter()
            .enumerate()
            .map(|(idx, (label, description))| {
                let prefix = if idx == selected { '›' } else { ' ' };
                let number = idx + 1;
                GenericDisplayRow {
                    name: format!("{prefix} {number}. {label}"),
                    description: Some(description.clone()),
                    ..Default::default()
                }
            })
            .collect()
    }

    pub(super) fn is_question_answered(&self, idx: usize, _current_text: &str) -> bool {
        let Some(question) = self.request.questions.get(idx) else {
            return false;
        };
        let Some(answer) = self.answers.get(idx) else {
            return false;
        };
        let has_options = question
            .options
            .as_ref()
            .is_some_and(|options| !options.is_empty());
        if has_options {
            answer.options_state.selected_idx.is_some() && answer.answer_committed
        } else {
            answer.answer_committed
        }
    }

    /// Count questions that would submit an empty answer list.
    pub(super) fn unanswered_count(&self) -> usize {
        let current_text = self.composer.current_text();
        self.request
            .questions
            .iter()
            .enumerate()
            .filter(|(idx, _question)| !self.is_question_answered(*idx, &current_text))
            .count()
    }

    /// Compute the preferred notes input height for the current question.
    pub(super) fn notes_input_height(&self, width: u16) -> u16 {
        let min_height = MIN_COMPOSER_HEIGHT;
        self.composer
            .desired_height(width.max(1))
            .clamp(min_height, min_height.saturating_add(5))
    }

    pub(super) fn apply_submission_to_draft(
        &mut self,
        text: String,
        text_elements: Vec<TextElement>,
    ) {
        let local_image_paths = self
            .composer
            .local_images()
            .into_iter()
            .map(|img| img.path)
            .collect::<Vec<_>>();
        if let Some(answer) = self.current_answer_mut() {
            answer.draft = ComposerDraft {
                text: text.clone(),
                text_elements: text_elements.clone(),
                local_image_paths: local_image_paths.clone(),
                pending_pastes: Vec::new(),
            };
        }
        self.composer
            .set_text_content(text, text_elements, local_image_paths);
        self.composer.move_cursor_to_end();
        self.composer.set_footer_hint_override(Some(Vec::new()));
    }

    pub(super) fn apply_submission_draft(&mut self, draft: ComposerDraft) {
        if let Some(answer) = self.current_answer_mut() {
            answer.draft = draft.clone();
        }
        self.composer
            .set_text_content(draft.text, draft.text_elements, draft.local_image_paths);
        self.composer.set_pending_pastes(draft.pending_pastes);
        self.composer.move_cursor_to_end();
        self.composer.set_footer_hint_override(Some(Vec::new()));
    }

    pub(super) fn handle_composer_input_result(&mut self, result: InputResult) -> bool {
        match result {
            InputResult::Submitted {
                text,
                text_elements,
            }
            | InputResult::Queued {
                text,
                text_elements,
            } => {
                if self.has_options()
                    && matches!(self.focus, Focus::Notes)
                    && !text.trim().is_empty()
                {
                    let options_len = self.options_len();
                    if let Some(answer) = self.current_answer_mut() {
                        answer.options_state.clamp_selection(options_len);
                    }
                }
                if self.has_options() {
                    if let Some(answer) = self.current_answer_mut() {
                        answer.answer_committed = true;
                    }
                } else if let Some(answer) = self.current_answer_mut() {
                    answer.answer_committed = !text.trim().is_empty();
                }
                let draft_override = self.pending_submission_draft.take();
                if let Some(draft) = draft_override {
                    self.apply_submission_draft(draft);
                } else {
                    self.apply_submission_to_draft(text, text_elements);
                }
                self.go_next_or_submit();
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_confirm_unanswered_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Release {
            return;
        }
        let Some(state) = self.confirm_unanswered.as_mut() else {
            return;
        };

        match key_event.code {
            KeyCode::Esc | KeyCode::Backspace => {
                self.close_unanswered_confirmation();
                if let Some(idx) = self.first_unanswered_index() {
                    self.jump_to_question(idx);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.move_up_wrap(/*len*/ 2);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.move_down_wrap(/*len*/ 2);
            }
            KeyCode::Enter => {
                let selected = state.selected_idx.unwrap_or(0);
                self.close_unanswered_confirmation();
                if selected == 0 {
                    self.submit_answers();
                } else if let Some(idx) = self.first_unanswered_index() {
                    self.jump_to_question(idx);
                }
            }
            KeyCode::Char('1') | KeyCode::Char('2') => {
                let idx = if matches!(key_event.code, KeyCode::Char('1')) {
                    0
                } else {
                    1
                };
                state.selected_idx = Some(idx);
            }
            _ => {}
        }
    }
}

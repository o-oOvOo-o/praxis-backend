use super::*;

pub(crate) struct McpServerElicitationOverlay {
    pub(super) app_event_tx: AppEventSender,
    pub(super) request: McpServerElicitationFormRequest,
    pub(super) queue: VecDeque<McpServerElicitationFormRequest>,
    pub(super) composer: ChatComposer,
    pub(super) answers: Vec<McpServerElicitationAnswerState>,
    pub(super) current_idx: usize,
    pub(super) done: bool,
    pub(super) validation_error: Option<String>,
}

impl McpServerElicitationOverlay {
    pub(crate) fn new(
        request: McpServerElicitationFormRequest,
        app_event_tx: AppEventSender,
        has_input_focus: bool,
        enhanced_keys_supported: bool,
        disable_paste_burst: bool,
    ) -> Self {
        let mut composer = ChatComposer::new_with_config(
            has_input_focus,
            app_event_tx.clone(),
            enhanced_keys_supported,
            ANSWER_PLACEHOLDER.to_string(),
            disable_paste_burst,
            ChatComposerConfig::plain_text(),
        );
        composer.set_footer_hint_override(Some(Vec::new()));
        let mut overlay = Self {
            app_event_tx,
            request,
            queue: VecDeque::new(),
            composer,
            answers: Vec::new(),
            current_idx: 0,
            done: false,
            validation_error: None,
        };
        overlay.reset_for_request();
        overlay.restore_current_draft();
        overlay
    }

    pub(super) fn reset_for_request(&mut self) {
        self.answers = self
            .request
            .fields
            .iter()
            .map(|field| {
                let mut selection = ScrollState::new();
                let (draft, answer_committed) = match &field.input {
                    McpServerElicitationFieldInput::Select { default_idx, .. } => {
                        selection.selected_idx = default_idx.or(Some(0));
                        (ComposerDraft::default(), default_idx.is_some())
                    }
                    McpServerElicitationFieldInput::Text { .. } => {
                        (ComposerDraft::default(), false)
                    }
                };
                McpServerElicitationAnswerState {
                    selection,
                    draft,
                    answer_committed,
                }
            })
            .collect();
        self.current_idx = 0;
        self.validation_error = None;
        self.composer
            .set_text_content(String::new(), Vec::new(), Vec::new());
    }

    pub(super) fn field_count(&self) -> usize {
        self.request.fields.len()
    }

    pub(super) fn current_index(&self) -> usize {
        self.current_idx
    }

    pub(super) fn current_field(&self) -> Option<&McpServerElicitationField> {
        self.request.fields.get(self.current_index())
    }

    pub(super) fn current_answer(&self) -> Option<&McpServerElicitationAnswerState> {
        self.answers.get(self.current_index())
    }

    pub(super) fn current_answer_mut(&mut self) -> Option<&mut McpServerElicitationAnswerState> {
        let idx = self.current_idx;
        self.answers.get_mut(idx)
    }

    pub(super) fn capture_composer_draft(&self) -> ComposerDraft {
        ComposerDraft {
            text: self.composer.current_text(),
            text_elements: self.composer.text_elements(),
            local_image_paths: self
                .composer
                .local_images()
                .into_iter()
                .map(|img| img.path)
                .collect(),
            pending_pastes: self.composer.pending_pastes(),
        }
    }

    pub(super) fn restore_current_draft(&mut self) {
        self.composer
            .set_placeholder_text(self.answer_placeholder().to_string());
        self.composer.set_footer_hint_override(Some(Vec::new()));
        if self.current_field_is_select() {
            self.composer
                .set_text_content(String::new(), Vec::new(), Vec::new());
            self.composer.move_cursor_to_end();
            return;
        }
        let Some(answer) = self.current_answer() else {
            self.composer
                .set_text_content(String::new(), Vec::new(), Vec::new());
            self.composer.move_cursor_to_end();
            return;
        };
        let draft = answer.draft.clone();
        self.composer
            .set_text_content(draft.text, draft.text_elements, draft.local_image_paths);
        self.composer.set_pending_pastes(draft.pending_pastes);
        self.composer.move_cursor_to_end();
    }

    pub(super) fn save_current_draft(&mut self) {
        if self.current_field_is_select() {
            return;
        }
        let draft = self.capture_composer_draft();
        if let Some(answer) = self.current_answer_mut() {
            if answer.answer_committed && answer.draft != draft {
                answer.answer_committed = false;
            }
            answer.draft = draft;
        }
    }

    pub(super) fn clear_current_draft(&mut self) {
        if self.current_field_is_select() {
            return;
        }
        if let Some(answer) = self.current_answer_mut() {
            answer.draft = ComposerDraft::default();
            answer.answer_committed = false;
        }
        self.composer
            .set_text_content(String::new(), Vec::new(), Vec::new());
        self.composer.move_cursor_to_end();
    }

    pub(super) fn answer_placeholder(&self) -> &'static str {
        self.current_field().map_or(ANSWER_PLACEHOLDER, |field| {
            if field.required {
                ANSWER_PLACEHOLDER
            } else {
                OPTIONAL_ANSWER_PLACEHOLDER
            }
        })
    }

    pub(super) fn current_field_is_select(&self) -> bool {
        matches!(
            self.current_field().map(|field| &field.input),
            Some(McpServerElicitationFieldInput::Select { .. })
        )
    }

    pub(super) fn current_field_is_secret(&self) -> bool {
        matches!(
            self.current_field().map(|field| &field.input),
            Some(McpServerElicitationFieldInput::Text { secret: true })
        )
    }

    pub(super) fn selected_option_index(&self) -> Option<usize> {
        self.current_answer()
            .and_then(|answer| answer.selection.selected_idx)
    }

    pub(super) fn options_len(&self) -> usize {
        self.current_options().len()
    }

    pub(super) fn current_options(&self) -> &[McpServerElicitationOption] {
        match self.current_field().map(|field| &field.input) {
            Some(McpServerElicitationFieldInput::Select { options, .. }) => options.as_slice(),
            _ => &[],
        }
    }

    pub(super) fn option_rows(&self) -> Vec<GenericDisplayRow> {
        let selected_idx = self.selected_option_index();
        self.current_options()
            .iter()
            .enumerate()
            .map(|(idx, option)| {
                let prefix = if selected_idx.is_some_and(|selected| selected == idx) {
                    '›'
                } else {
                    ' '
                };
                let number = idx + 1;
                let prefix_label = format!("{prefix} {number}. ");
                let wrap_indent = UnicodeWidthStr::width(prefix_label.as_str());
                GenericDisplayRow {
                    name: format!("{prefix_label}{}", option.label),
                    description: option.description.clone(),
                    wrap_indent: Some(wrap_indent),
                    ..Default::default()
                }
            })
            .collect()
    }

    pub(super) fn wrapped_prompt_lines(&self, width: u16) -> Vec<String> {
        textwrap::wrap(&self.current_prompt_text(), width.max(1) as usize)
            .into_iter()
            .map(|line| line.to_string())
            .collect()
    }

    pub(super) fn current_prompt_text(&self) -> String {
        let request_message = format_tool_approval_display_message(
            &self.request.message,
            &self.request.approval_display_params,
        );
        let Some(field) = self.current_field() else {
            return request_message;
        };
        let mut sections = Vec::new();
        if !request_message.trim().is_empty() {
            sections.push(request_message);
        }
        let field_prompt = if field.label.trim().is_empty()
            || field.prompt.trim().is_empty()
            || field.label == field.prompt
        {
            if field.prompt.trim().is_empty() {
                field.label.clone()
            } else {
                field.prompt.clone()
            }
        } else {
            format!("{}\n{}", field.label, field.prompt)
        };
        if !field_prompt.trim().is_empty() {
            sections.push(field_prompt);
        }
        sections.join("\n\n")
    }

    pub(super) fn footer_tips(&self) -> Vec<FooterTip> {
        let mut tips = Vec::new();
        let is_last_field = self.current_index().saturating_add(1) >= self.field_count();
        if self.current_field_is_select() {
            if self.field_count() == 1 {
                tips.push(FooterTip::highlighted("enter to submit"));
            } else if is_last_field {
                tips.push(FooterTip::highlighted("enter to submit all"));
            } else {
                tips.push(FooterTip::new("enter to submit answer"));
            }
        } else if self.field_count() == 1 {
            tips.push(FooterTip::highlighted("enter to submit"));
        } else if is_last_field {
            tips.push(FooterTip::highlighted("enter to submit all"));
        } else {
            tips.push(FooterTip::new("enter to submit answer"));
        }
        if self.field_count() > 1 {
            if self.current_field_is_select() {
                tips.push(FooterTip::new("←/→ to navigate fields"));
            } else {
                tips.push(FooterTip::new("ctrl + p / ctrl + n change field"));
            }
        }
        tips.push(FooterTip::new("esc to cancel"));
        tips
    }

    pub(super) fn footer_tip_lines(&self, width: u16) -> Vec<Vec<FooterTip>> {
        let mut tips = Vec::new();
        if let Some(error) = self.validation_error.as_ref() {
            tips.push(FooterTip::highlighted(error.clone()));
        }
        tips.extend(self.footer_tips());
        wrap_footer_tips(width, tips)
    }

    pub(super) fn options_required_height(&self, width: u16) -> u16 {
        let rows = self.option_rows();
        if rows.is_empty() {
            return 0;
        }
        let mut state = self
            .current_answer()
            .map(|answer| answer.selection)
            .unwrap_or_default();
        if state.selected_idx.is_none() {
            state.selected_idx = Some(0);
        }
        measure_rows_height(&rows, &state, rows.len(), width.max(1))
    }

    pub(super) fn input_height(&self, width: u16) -> u16 {
        if self.current_field_is_select() {
            return self.options_required_height(width);
        }
        self.composer
            .desired_height(width.max(1))
            .clamp(MIN_COMPOSER_HEIGHT, MIN_COMPOSER_HEIGHT.saturating_add(5))
    }

    pub(super) fn move_field(&mut self, next: bool) {
        let len = self.field_count();
        if len == 0 {
            return;
        }
        self.save_current_draft();
        let offset = if next { 1 } else { len.saturating_sub(1) };
        self.current_idx = (self.current_idx + offset) % len;
        self.validation_error = None;
        self.restore_current_draft();
    }

    pub(super) fn jump_to_field(&mut self, idx: usize) {
        if idx >= self.field_count() {
            return;
        }
        self.save_current_draft();
        self.current_idx = idx;
        self.restore_current_draft();
    }

    pub(super) fn field_value(&self, idx: usize) -> Option<Value> {
        let field = self.request.fields.get(idx)?;
        let answer = self.answers.get(idx)?;
        match &field.input {
            McpServerElicitationFieldInput::Select { options, .. } => {
                if !answer.answer_committed {
                    return None;
                }
                let selected_idx = answer.selection.selected_idx?;
                options.get(selected_idx).map(|option| option.value.clone())
            }
            McpServerElicitationFieldInput::Text { .. } => {
                if !answer.answer_committed {
                    return None;
                }
                let text = answer.draft.text_with_pending();
                let text = text.trim();
                (!text.is_empty()).then(|| Value::String(text.to_string()))
            }
        }
    }

    pub(super) fn required_unanswered_count(&self) -> usize {
        self.request
            .fields
            .iter()
            .enumerate()
            .filter(|(idx, field)| field.required && self.field_value(*idx).is_none())
            .count()
    }

    pub(super) fn first_required_unanswered_index(&self) -> Option<usize> {
        self.request
            .fields
            .iter()
            .enumerate()
            .find(|(idx, field)| field.required && self.field_value(*idx).is_none())
            .map(|(idx, _)| idx)
    }

    pub(super) fn is_current_field_answered(&self) -> bool {
        self.field_value(self.current_index()).is_some()
    }

    pub(super) fn option_index_for_digit(&self, ch: char) -> Option<usize> {
        let digit = ch.to_digit(10)?;
        if digit == 0 {
            return None;
        }
        let idx = (digit - 1) as usize;
        (idx < self.options_len()).then_some(idx)
    }

    pub(super) fn select_current_option(&mut self, committed: bool) {
        let options_len = self.options_len();
        if let Some(answer) = self.current_answer_mut() {
            answer.selection.clamp_selection(options_len);
            answer.answer_committed = committed;
        }
    }

    pub(super) fn clear_selection(&mut self) {
        if let Some(answer) = self.current_answer_mut() {
            answer.selection.reset();
            answer.answer_committed = false;
        }
    }

    pub(super) fn dispatch_cancel(&self) {
        self.app_event_tx.resolve_elicitation(
            self.request.thread_id,
            self.request.server_name.clone(),
            self.request.request_id.clone(),
            ElicitationAction::Cancel,
            /*content*/ None,
            /*meta*/ None,
        );
    }

    pub(super) fn submit_answers(&mut self) {
        self.save_current_draft();
        if let Some(idx) = self.first_required_unanswered_index() {
            self.validation_error = Some("Answer required fields before submitting.".to_string());
            self.jump_to_field(idx);
            return;
        }
        self.validation_error = None;
        if self.request.response_mode == McpServerElicitationResponseMode::ApprovalAction {
            let (decision, meta) =
                match self.field_value(/*idx*/ 0).as_ref().and_then(Value::as_str) {
                    Some(APPROVAL_ACCEPT_ONCE_VALUE) => (ElicitationAction::Accept, None),
                    Some(APPROVAL_ACCEPT_SESSION_VALUE) => (
                        ElicitationAction::Accept,
                        Some(serde_json::json!({
                            APPROVAL_PERSIST_KEY: APPROVAL_PERSIST_SESSION_VALUE,
                        })),
                    ),
                    Some(APPROVAL_ACCEPT_ALWAYS_VALUE) => (
                        ElicitationAction::Accept,
                        Some(serde_json::json!({
                            APPROVAL_PERSIST_KEY: APPROVAL_PERSIST_ALWAYS_VALUE,
                        })),
                    ),
                    Some(APPROVAL_DECLINE_VALUE) => (ElicitationAction::Decline, None),
                    Some(APPROVAL_CANCEL_VALUE) => (ElicitationAction::Cancel, None),
                    _ => (ElicitationAction::Cancel, None),
                };
            self.app_event_tx.resolve_elicitation(
                self.request.thread_id,
                self.request.server_name.clone(),
                self.request.request_id.clone(),
                decision,
                /*content*/ None,
                meta,
            );
            if let Some(next) = self.queue.pop_front() {
                self.request = next;
                self.reset_for_request();
                self.restore_current_draft();
            } else {
                self.done = true;
            }
            return;
        }
        let content = self
            .request
            .fields
            .iter()
            .enumerate()
            .filter_map(|(idx, field)| self.field_value(idx).map(|value| (field.id.clone(), value)))
            .collect::<serde_json::Map<_, _>>();
        self.app_event_tx.resolve_elicitation(
            self.request.thread_id,
            self.request.server_name.clone(),
            self.request.request_id.clone(),
            ElicitationAction::Accept,
            Some(Value::Object(content)),
            /*meta*/ None,
        );
        if let Some(next) = self.queue.pop_front() {
            self.request = next;
            self.reset_for_request();
            self.restore_current_draft();
        } else {
            self.done = true;
        }
    }

    pub(super) fn go_next_or_submit(&mut self) {
        if self.current_index() + 1 >= self.field_count() {
            self.submit_answers();
        } else {
            self.move_field(/*next*/ true);
        }
    }

    pub(super) fn apply_submission_to_draft(&mut self, text: String, text_elements: Vec<TextElement>) {
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
            answer.answer_committed = !text.trim().is_empty();
        }
        self.composer
            .set_text_content(text, text_elements, local_image_paths);
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
                self.apply_submission_to_draft(text, text_elements);
                self.validation_error = None;
                self.go_next_or_submit();
                true
            }
            _ => false,
        }
    }
}

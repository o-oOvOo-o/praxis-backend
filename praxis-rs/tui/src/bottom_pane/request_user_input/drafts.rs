use super::*;

impl RequestUserInputOverlay {
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

    pub(super) fn save_current_draft(&mut self) {
        let draft = self.capture_composer_draft();
        let notes_empty = draft.text.trim().is_empty();
        if let Some(answer) = self.current_answer_mut() {
            if answer.answer_committed && answer.draft != draft {
                answer.answer_committed = false;
            }
            answer.draft = draft;
            if !notes_empty {
                answer.notes_visible = true;
            }
        }
    }

    pub(super) fn restore_current_draft(&mut self) {
        self.composer
            .set_placeholder_text(self.notes_placeholder().to_string());
        self.composer.set_footer_hint_override(Some(Vec::new()));
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

    pub(super) fn notes_placeholder(&self) -> &'static str {
        if self.has_options() && self.selected_option_index().is_none() {
            SELECT_OPTION_PLACEHOLDER
        } else if self.has_options() {
            NOTES_PLACEHOLDER
        } else {
            ANSWER_PLACEHOLDER
        }
    }

    pub(super) fn sync_composer_placeholder(&mut self) {
        self.composer
            .set_placeholder_text(self.notes_placeholder().to_string());
    }

    pub(super) fn clear_notes_draft(&mut self) {
        if let Some(answer) = self.current_answer_mut() {
            answer.draft = ComposerDraft::default();
            answer.answer_committed = false;
            answer.notes_visible = true;
        }
        self.pending_submission_draft = None;
        self.composer
            .set_text_content(String::new(), Vec::new(), Vec::new());
        self.composer.move_cursor_to_end();
        self.sync_composer_placeholder();
    }

    pub(super) fn footer_tips(&self) -> Vec<FooterTip> {
        let mut tips = Vec::new();
        let notes_visible = self.notes_ui_visible();
        if self.has_options() {
            if self.selected_option_index().is_some() && !notes_visible {
                tips.push(FooterTip::highlighted("tab to add notes"));
            }
            if self.selected_option_index().is_some() && notes_visible {
                tips.push(FooterTip::new("tab or esc to clear notes"));
            }
        }

        let question_count = self.question_count();
        let is_last_question = self.current_index().saturating_add(1) >= question_count;
        let enter_tip = if question_count == 1 {
            FooterTip::highlighted("enter to submit answer")
        } else if is_last_question {
            FooterTip::highlighted("enter to submit all")
        } else {
            FooterTip::new("enter to submit answer")
        };
        tips.push(enter_tip);
        if question_count > 1 {
            if self.has_options() && !self.focus_is_notes() {
                tips.push(FooterTip::new("←/→ to navigate questions"));
            } else if !self.has_options() {
                tips.push(FooterTip::new("ctrl + p / ctrl + n change question"));
            }
        }
        if !(self.has_options() && notes_visible) {
            tips.push(FooterTip::new("esc to interrupt"));
        }
        tips
    }

    pub(super) fn footer_tip_lines(&self, width: u16) -> Vec<Vec<FooterTip>> {
        self.wrap_footer_tips(width, self.footer_tips())
    }

    pub(super) fn footer_tip_lines_with_prefix(
        &self,
        width: u16,
        prefix: Option<FooterTip>,
    ) -> Vec<Vec<FooterTip>> {
        let mut tips = Vec::new();
        if let Some(prefix) = prefix {
            tips.push(prefix);
        }
        tips.extend(self.footer_tips());
        self.wrap_footer_tips(width, tips)
    }

    pub(super) fn wrap_footer_tips(&self, width: u16, tips: Vec<FooterTip>) -> Vec<Vec<FooterTip>> {
        let max_width = width.max(1) as usize;
        let separator_width = UnicodeWidthStr::width(TIP_SEPARATOR);
        if tips.is_empty() {
            return vec![Vec::new()];
        }

        let mut lines: Vec<Vec<FooterTip>> = Vec::new();
        let mut current: Vec<FooterTip> = Vec::new();
        let mut used = 0usize;

        for tip in tips {
            let tip_width = UnicodeWidthStr::width(tip.text.as_str()).min(max_width);
            let extra = if current.is_empty() {
                tip_width
            } else {
                separator_width.saturating_add(tip_width)
            };
            if !current.is_empty() && used.saturating_add(extra) > max_width {
                lines.push(current);
                current = Vec::new();
                used = 0;
            }
            if current.is_empty() {
                used = tip_width;
            } else {
                used = used
                    .saturating_add(separator_width)
                    .saturating_add(tip_width);
            }
            current.push(tip);
        }

        if current.is_empty() {
            lines.push(Vec::new());
        } else {
            lines.push(current);
        }
        lines
    }

    pub(super) fn footer_required_height(&self, width: u16) -> u16 {
        self.footer_tip_lines(width).len() as u16
    }
}

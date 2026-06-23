use crate::scrollable_diff::ScrollableDiff;
use praxis_cloud_tasks_client::AttemptStatus;
use praxis_cloud_tasks_client::TaskId;

pub struct DiffOverlay {
    pub title: String,
    pub task_id: TaskId,
    pub sd: ScrollableDiff,
    pub base_can_apply: bool,
    pub diff_lines: Vec<String>,
    pub text_lines: Vec<String>,
    pub prompt: Option<String>,
    pub attempts: Vec<AttemptView>,
    pub selected_attempt: usize,
    pub current_view: DetailView,
    pub base_turn_id: Option<String>,
    pub sibling_turn_ids: Vec<String>,
    pub attempt_total_hint: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct AttemptView {
    pub turn_id: Option<String>,
    pub status: AttemptStatus,
    pub attempt_placement: Option<i64>,
    pub diff_lines: Vec<String>,
    pub text_lines: Vec<String>,
    pub prompt: Option<String>,
    pub diff_raw: Option<String>,
}

impl AttemptView {
    pub fn has_diff(&self) -> bool {
        !self.diff_lines.is_empty()
    }

    pub fn has_text(&self) -> bool {
        !self.text_lines.is_empty() || self.prompt.is_some()
    }
}

impl DiffOverlay {
    pub fn new(task_id: TaskId, title: String, attempt_total_hint: Option<usize>) -> Self {
        let mut sd = ScrollableDiff::new();
        sd.set_content(Vec::new());
        Self {
            title,
            task_id,
            sd,
            base_can_apply: false,
            diff_lines: Vec::new(),
            text_lines: Vec::new(),
            prompt: None,
            attempts: vec![AttemptView::default()],
            selected_attempt: 0,
            current_view: DetailView::Prompt,
            base_turn_id: None,
            sibling_turn_ids: Vec::new(),
            attempt_total_hint,
        }
    }

    pub fn current_attempt(&self) -> Option<&AttemptView> {
        self.attempts.get(self.selected_attempt)
    }

    pub fn base_attempt_mut(&mut self) -> &mut AttemptView {
        if self.attempts.is_empty() {
            self.attempts.push(AttemptView::default());
        }
        &mut self.attempts[0]
    }

    pub fn set_view(&mut self, view: DetailView) {
        self.current_view = view;
        self.apply_selection_to_fields();
    }

    pub fn expected_attempts(&self) -> Option<usize> {
        self.attempt_total_hint.or({
            if self.attempts.is_empty() {
                None
            } else {
                Some(self.attempts.len())
            }
        })
    }

    pub fn attempt_count(&self) -> usize {
        self.attempts.len()
    }

    pub fn attempt_display_total(&self) -> usize {
        self.expected_attempts()
            .unwrap_or_else(|| self.attempts.len().max(1))
    }

    pub fn step_attempt(&mut self, delta: isize) -> bool {
        let total = self.attempts.len();
        if total <= 1 {
            return false;
        }
        let total_isize = total as isize;
        let current = self.selected_attempt as isize;
        let mut next = current + delta;
        next = ((next % total_isize) + total_isize) % total_isize;
        let next = next as usize;
        self.selected_attempt = next;
        self.apply_selection_to_fields();
        true
    }

    pub fn current_can_apply(&self) -> bool {
        matches!(self.current_view, DetailView::Diff)
            && self
                .current_attempt()
                .and_then(|attempt| attempt.diff_raw.as_ref())
                .map(|diff| !diff.is_empty())
                .unwrap_or(false)
    }

    pub fn apply_selection_to_fields(&mut self) {
        let (diff_lines, text_lines, prompt) = if let Some(attempt) = self.current_attempt() {
            (
                attempt.diff_lines.clone(),
                attempt.text_lines.clone(),
                attempt.prompt.clone(),
            )
        } else {
            self.diff_lines.clear();
            self.text_lines.clear();
            self.prompt = None;
            self.sd.set_content(vec!["<loading attempt>".to_string()]);
            return;
        };

        self.diff_lines = diff_lines.clone();
        self.text_lines = text_lines.clone();
        self.prompt = prompt;

        match self.current_view {
            DetailView::Diff => {
                if diff_lines.is_empty() {
                    self.sd.set_content(vec!["<no diff available>".to_string()]);
                } else {
                    self.sd.set_content(diff_lines);
                }
            }
            DetailView::Prompt => {
                if text_lines.is_empty() {
                    self.sd.set_content(vec!["<no output>".to_string()]);
                } else {
                    self.sd.set_content(text_lines);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailView {
    Diff,
    Prompt,
}

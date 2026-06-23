use super::ApplyModalState;
use super::BestOfModalState;
use super::DiffOverlay;
use super::EnvModalState;
use super::EnvironmentRow;
use praxis_cloud_tasks_client::TaskSummary;
use std::time::Instant;

#[derive(Default)]
pub struct App {
    pub tasks: Vec<TaskSummary>,
    pub selected: usize,
    pub status: String,
    pub diff_overlay: Option<DiffOverlay>,
    pub spinner_start: Option<Instant>,
    pub refresh_inflight: bool,
    pub details_inflight: bool,
    pub env_filter: Option<String>,
    pub env_modal: Option<EnvModalState>,
    pub apply_modal: Option<ApplyModalState>,
    pub best_of_modal: Option<BestOfModalState>,
    pub environments: Vec<EnvironmentRow>,
    pub env_last_loaded: Option<Instant>,
    pub env_loading: bool,
    pub env_error: Option<String>,
    pub new_task: Option<crate::new_task::NewTaskPage>,
    pub best_of_n: usize,
    pub apply_preflight_inflight: bool,
    pub apply_inflight: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            selected: 0,
            status: "Press r to refresh".to_string(),
            diff_overlay: None,
            spinner_start: None,
            refresh_inflight: false,
            details_inflight: false,
            env_filter: None,
            env_modal: None,
            apply_modal: None,
            best_of_modal: None,
            environments: Vec::new(),
            env_last_loaded: None,
            env_loading: false,
            env_error: None,
            new_task: None,
            best_of_n: 1,
            apply_preflight_inflight: false,
            apply_inflight: false,
        }
    }

    pub fn next(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.tasks.len().saturating_sub(1));
    }

    pub fn prev(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
}

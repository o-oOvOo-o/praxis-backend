use praxis_cloud_tasks_client::TaskId;

#[derive(Clone, Debug, Default)]
pub struct BestOfModalState {
    pub selected: usize,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum ApplyResultLevel {
    Success,
    Partial,
    Error,
}

#[derive(Clone, Debug)]
pub struct ApplyModalState {
    pub task_id: TaskId,
    pub title: String,
    pub result_message: Option<String>,
    pub result_level: Option<ApplyResultLevel>,
    pub skipped_paths: Vec<String>,
    pub conflict_paths: Vec<String>,
    pub diff_override: Option<String>,
}

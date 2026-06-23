use super::ApplyResultLevel;
use super::EnvironmentRow;
use praxis_cloud_tasks_client::ApplyOutcome;
use praxis_cloud_tasks_client::AttemptStatus;
use praxis_cloud_tasks_client::CreatedTask;
use praxis_cloud_tasks_client::TaskId;
use praxis_cloud_tasks_client::TaskSummary;
use praxis_cloud_tasks_client::TurnAttempt;

#[derive(Debug)]
pub enum AppEvent {
    TasksLoaded {
        env: Option<String>,
        result: anyhow::Result<Vec<TaskSummary>>,
    },
    EnvironmentAutodetected(anyhow::Result<crate::env_detect::AutodetectSelection>),
    EnvironmentsLoaded(anyhow::Result<Vec<EnvironmentRow>>),
    DetailsDiffLoaded {
        id: TaskId,
        title: String,
        diff: String,
    },
    DetailsMessagesLoaded {
        id: TaskId,
        title: String,
        messages: Vec<String>,
        prompt: Option<String>,
        turn_id: Option<String>,
        sibling_turn_ids: Vec<String>,
        attempt_placement: Option<i64>,
        attempt_status: AttemptStatus,
    },
    DetailsFailed {
        id: TaskId,
        title: String,
        error: String,
    },
    AttemptsLoaded {
        id: TaskId,
        attempts: Vec<TurnAttempt>,
    },
    NewTaskSubmitted(Result<CreatedTask, String>),
    ApplyPreflightFinished {
        id: TaskId,
        title: String,
        message: String,
        level: ApplyResultLevel,
        skipped: Vec<String>,
        conflicts: Vec<String>,
    },
    ApplyDiffLoaded {
        id: TaskId,
        title: String,
        result: std::result::Result<Option<String>, String>,
    },
    ApplyFinished {
        id: TaskId,
        result: std::result::Result<ApplyOutcome, String>,
    },
}

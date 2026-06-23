use super::*;
use chrono::Utc;
use praxis_cloud_tasks_client::CloudTaskError;
use praxis_cloud_tasks_client::TaskId;
use praxis_cloud_tasks_client::TaskSummary;

struct FakeBackend {
    by_env: std::collections::HashMap<Option<String>, Vec<&'static str>>,
}

#[async_trait::async_trait]
impl praxis_cloud_tasks_client::CloudBackend for FakeBackend {
    async fn list_tasks(
        &self,
        env: Option<&str>,
        limit: Option<i64>,
        cursor: Option<&str>,
    ) -> praxis_cloud_tasks_client::Result<praxis_cloud_tasks_client::TaskListPage> {
        let key = env.map(str::to_string);
        let titles = self
            .by_env
            .get(&key)
            .cloned()
            .unwrap_or_else(|| vec!["default-a", "default-b"]);
        let mut out = Vec::new();
        for (i, title) in titles.into_iter().enumerate() {
            out.push(TaskSummary {
                id: TaskId(format!("T-{i}")),
                title: title.to_string(),
                status: praxis_cloud_tasks_client::TaskStatus::Ready,
                updated_at: Utc::now(),
                environment_id: env.map(str::to_string),
                environment_label: None,
                summary: praxis_cloud_tasks_client::DiffSummary::default(),
                is_review: false,
                attempt_total: Some(1),
            });
        }
        let max = limit.unwrap_or(i64::MAX).min(20);
        let mut limited = Vec::new();
        for task in out {
            if (limited.len() as i64) >= max {
                break;
            }
            limited.push(task);
        }
        Ok(praxis_cloud_tasks_client::TaskListPage {
            tasks: limited,
            cursor: cursor.map(str::to_string),
        })
    }

    async fn get_task_summary(&self, id: TaskId) -> praxis_cloud_tasks_client::Result<TaskSummary> {
        self.list_tasks(/*env*/ None, /*limit*/ None, /*cursor*/ None)
            .await?
            .tasks
            .into_iter()
            .find(|task| task.id == id)
            .ok_or_else(|| CloudTaskError::Msg(format!("Task {} not found", id.0)))
    }

    async fn get_task_diff(
        &self,
        _id: TaskId,
    ) -> praxis_cloud_tasks_client::Result<Option<String>> {
        Err(CloudTaskError::Unimplemented("not used in test"))
    }

    async fn get_task_messages(
        &self,
        _id: TaskId,
    ) -> praxis_cloud_tasks_client::Result<Vec<String>> {
        Ok(vec![])
    }

    async fn get_task_text(
        &self,
        _id: TaskId,
    ) -> praxis_cloud_tasks_client::Result<praxis_cloud_tasks_client::TaskText> {
        Ok(praxis_cloud_tasks_client::TaskText {
            prompt: Some("Example prompt".to_string()),
            messages: Vec::new(),
            turn_id: Some("fake-turn".to_string()),
            sibling_turn_ids: Vec::new(),
            attempt_placement: Some(0),
            attempt_status: praxis_cloud_tasks_client::AttemptStatus::Completed,
        })
    }

    async fn list_sibling_attempts(
        &self,
        _task: TaskId,
        _turn_id: String,
    ) -> praxis_cloud_tasks_client::Result<Vec<praxis_cloud_tasks_client::TurnAttempt>> {
        Ok(Vec::new())
    }

    async fn apply_task(
        &self,
        _id: TaskId,
        _diff_override: Option<String>,
    ) -> praxis_cloud_tasks_client::Result<praxis_cloud_tasks_client::ApplyOutcome> {
        Err(CloudTaskError::Unimplemented("not used in test"))
    }

    async fn apply_task_preflight(
        &self,
        _id: TaskId,
        _diff_override: Option<String>,
    ) -> praxis_cloud_tasks_client::Result<praxis_cloud_tasks_client::ApplyOutcome> {
        Err(CloudTaskError::Unimplemented("not used in test"))
    }

    async fn create_task(
        &self,
        _env_id: &str,
        _prompt: &str,
        _git_ref: &str,
        _qa_mode: bool,
        _best_of_n: usize,
    ) -> praxis_cloud_tasks_client::Result<praxis_cloud_tasks_client::CreatedTask> {
        Err(CloudTaskError::Unimplemented("not used in test"))
    }
}

#[tokio::test]
async fn load_tasks_uses_env_parameter() {
    let mut by_env = std::collections::HashMap::new();
    by_env.insert(None, vec!["root-1", "root-2"]);
    by_env.insert(Some("env-A".to_string()), vec!["A-1"]);
    by_env.insert(Some("env-B".to_string()), vec!["B-1", "B-2", "B-3"]);
    let backend = FakeBackend { by_env };

    let root = load_tasks(&backend, /*env*/ None).await.unwrap();
    assert_eq!(root.len(), 2);
    assert_eq!(root[0].title, "root-1");

    let a = load_tasks(&backend, Some("env-A")).await.unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].title, "A-1");

    let b = load_tasks(&backend, Some("env-B")).await.unwrap();
    assert_eq!(b.len(), 3);
    assert_eq!(b[2].title, "B-3");
}

use praxis_cloud_tasks_client::CloudBackend;
use praxis_cloud_tasks_client::TaskSummary;
use std::time::Duration;

pub async fn load_tasks(
    backend: &dyn CloudBackend,
    env: Option<&str>,
) -> anyhow::Result<Vec<TaskSummary>> {
    let tasks = tokio::time::timeout(
        Duration::from_secs(5),
        backend.list_tasks(env, Some(20), /*cursor*/ None),
    )
    .await??;
    Ok(tasks
        .tasks
        .into_iter()
        .filter(|task| !task.is_review)
        .collect())
}

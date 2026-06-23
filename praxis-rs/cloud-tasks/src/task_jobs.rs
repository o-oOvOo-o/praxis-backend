use crate::app;
use crate::command_support::resolve_git_ref;
use praxis_cloud_tasks_client::CloudBackend;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) fn spawn_task_refresh(
    backend: Arc<dyn CloudBackend>,
    tx: UnboundedSender<app::AppEvent>,
    env: Option<String>,
) {
    tokio::spawn(async move {
        let result = app::load_tasks(&*backend, env.as_deref()).await;
        let _ = tx.send(app::AppEvent::TasksLoaded { env, result });
    });
}

pub(crate) fn spawn_new_task_submit(
    backend: Arc<dyn CloudBackend>,
    tx: UnboundedSender<app::AppEvent>,
    env: String,
    text: String,
    best_of_n: usize,
) {
    tokio::spawn(async move {
        let git_ref = resolve_git_ref(/*branch_override*/ None).await;
        let result = CloudBackend::create_task(
            &*backend, &env, &text, &git_ref, /*qa_mode*/ false, best_of_n,
        )
        .await;
        let event = match result {
            Ok(created) => app::AppEvent::NewTaskSubmitted(Ok(created)),
            Err(error) => app::AppEvent::NewTaskSubmitted(Err(format!("{error}"))),
        };
        let _ = tx.send(event);
    });
}

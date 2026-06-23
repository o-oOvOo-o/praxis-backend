use crate::app;
use crate::util::append_error_log;
use praxis_cloud_tasks_client::CloudBackend;
use praxis_cloud_tasks_client::TaskId;
use praxis_cloud_tasks_client::TaskText;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) fn spawn_attempts_load(
    backend: Arc<dyn CloudBackend>,
    tx: UnboundedSender<app::AppEvent>,
    task_id: TaskId,
    turn_id: String,
) {
    tokio::spawn(async move {
        match CloudBackend::list_sibling_attempts(&*backend, task_id.clone(), turn_id).await {
            Ok(attempts) => {
                let _ = tx.send(app::AppEvent::AttemptsLoaded {
                    id: task_id,
                    attempts,
                });
            }
            Err(error) => {
                append_error_log(format!("attempts.load failed for {}: {error}", task_id.0));
            }
        }
    });
}

pub(crate) fn spawn_task_detail_loaders(
    backend: Arc<dyn CloudBackend>,
    tx: UnboundedSender<app::AppEvent>,
    task_id: TaskId,
    title: String,
) {
    spawn_diff_or_fallback_text(
        Arc::clone(&backend),
        tx.clone(),
        task_id.clone(),
        title.clone(),
    );
    spawn_text_only(backend, tx, task_id, title);
}

fn spawn_diff_or_fallback_text(
    backend: Arc<dyn CloudBackend>,
    tx: UnboundedSender<app::AppEvent>,
    task_id: TaskId,
    title: String,
) {
    tokio::spawn(async move {
        match CloudBackend::get_task_diff(&*backend, task_id.clone()).await {
            Ok(Some(diff)) => {
                let _ = tx.send(app::AppEvent::DetailsDiffLoaded {
                    id: task_id,
                    title,
                    diff,
                });
            }
            Ok(None) => send_text_or_failure(&*backend, &tx, task_id, title).await,
            Err(error) => {
                append_error_log(format!("get_task_diff failed for {}: {error}", task_id.0));
                send_text_or_failure(&*backend, &tx, task_id, title).await;
            }
        }
    });
}

fn spawn_text_only(
    backend: Arc<dyn CloudBackend>,
    tx: UnboundedSender<app::AppEvent>,
    task_id: TaskId,
    title: String,
) {
    tokio::spawn(async move {
        if let Ok(text) = CloudBackend::get_task_text(&*backend, task_id.clone()).await {
            let _ = tx.send(details_messages_event(task_id, title, text));
        }
    });
}

async fn send_text_or_failure(
    backend: &dyn CloudBackend,
    tx: &UnboundedSender<app::AppEvent>,
    task_id: TaskId,
    title: String,
) {
    match CloudBackend::get_task_text(backend, task_id.clone()).await {
        Ok(text) => {
            let _ = tx.send(details_messages_event(task_id, title, text));
        }
        Err(error) => {
            let _ = tx.send(app::AppEvent::DetailsFailed {
                id: task_id,
                title,
                error: format!("{error}"),
            });
        }
    }
}

fn details_messages_event(id: TaskId, title: String, text: TaskText) -> app::AppEvent {
    app::AppEvent::DetailsMessagesLoaded {
        id,
        title,
        messages: text.messages,
        prompt: text.prompt,
        turn_id: text.turn_id,
        sibling_turn_ids: text.sibling_turn_ids,
        attempt_placement: text.attempt_placement,
        attempt_status: text.attempt_status,
    }
}

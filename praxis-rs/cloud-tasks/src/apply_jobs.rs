use crate::app;
use praxis_cloud_tasks_client::ApplyStatus;
use praxis_cloud_tasks_client::CloudBackend;
use praxis_cloud_tasks_client::TaskId;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) struct ApplyJob {
    pub(crate) task_id: TaskId,
    pub(crate) diff_override: Option<String>,
}

pub(crate) fn spawn_preflight(
    app: &mut app::App,
    backend: &Arc<dyn CloudBackend>,
    tx: &UnboundedSender<app::AppEvent>,
    frame_tx: &UnboundedSender<Instant>,
    title: String,
    job: ApplyJob,
) -> bool {
    if app.apply_inflight {
        app.status = "An apply is already running; wait for it to finish first.".to_string();
        return false;
    }
    if app.apply_preflight_inflight {
        app.status = "A preflight is already running; wait for it to finish first.".to_string();
        return false;
    }

    app.apply_preflight_inflight = true;
    let _ = frame_tx.send(Instant::now() + Duration::from_millis(100));

    let backend = backend.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let ApplyJob {
            task_id,
            diff_override,
        } = job;
        let result =
            CloudBackend::apply_task_preflight(&*backend, task_id.clone(), diff_override).await;

        let event = match result {
            Ok(outcome) => {
                let level = level_from_status(outcome.status);
                app::AppEvent::ApplyPreflightFinished {
                    id: task_id,
                    title,
                    message: outcome.message,
                    level,
                    skipped: outcome.skipped_paths,
                    conflicts: outcome.conflict_paths,
                }
            }
            Err(e) => app::AppEvent::ApplyPreflightFinished {
                id: task_id,
                title,
                message: format!("Preflight failed: {e}"),
                level: app::ApplyResultLevel::Error,
                skipped: Vec::new(),
                conflicts: Vec::new(),
            },
        };

        let _ = tx.send(event);
    });

    true
}

pub(crate) fn spawn_apply(
    app: &mut app::App,
    backend: &Arc<dyn CloudBackend>,
    tx: &UnboundedSender<app::AppEvent>,
    frame_tx: &UnboundedSender<Instant>,
    job: ApplyJob,
) -> bool {
    if app.apply_inflight {
        app.status = "An apply is already running; wait for it to finish first.".to_string();
        return false;
    }
    if app.apply_preflight_inflight {
        app.status = "Finish the current preflight before starting another apply.".to_string();
        return false;
    }

    app.apply_inflight = true;
    let _ = frame_tx.send(Instant::now() + Duration::from_millis(100));

    let backend = backend.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let ApplyJob {
            task_id,
            diff_override,
        } = job;
        let result = CloudBackend::apply_task(&*backend, task_id.clone(), diff_override).await;

        let event = match result {
            Ok(outcome) => app::AppEvent::ApplyFinished {
                id: task_id,
                result: Ok(outcome),
            },
            Err(e) => app::AppEvent::ApplyFinished {
                id: task_id,
                result: Err(format!("{e}")),
            },
        };

        let _ = tx.send(event);
    });

    true
}

pub(crate) fn spawn_apply_diff_load(
    backend: Arc<dyn CloudBackend>,
    tx: UnboundedSender<app::AppEvent>,
    task_id: TaskId,
    title: String,
) {
    tokio::spawn(async move {
        let result = CloudBackend::get_task_diff(&*backend, task_id.clone())
            .await
            .map_err(|error| format!("{error}"));
        let _ = tx.send(app::AppEvent::ApplyDiffLoaded {
            id: task_id,
            title,
            result,
        });
    });
}

fn level_from_status(status: ApplyStatus) -> app::ApplyResultLevel {
    match status {
        ApplyStatus::Success => app::ApplyResultLevel::Success,
        ApplyStatus::Partial => app::ApplyResultLevel::Partial,
        ApplyStatus::Error => app::ApplyResultLevel::Error,
    }
}

use std::sync::Arc;

use praxis_git_utils::CreateGhostCommitOptions;
use praxis_git_utils::GhostSnapshotConfig;
use praxis_git_utils::GhostSnapshotReport;
use praxis_git_utils::GitToolingError;
use praxis_git_utils::create_ghost_commit_with_report;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::WarningEvent;
use praxis_utils_readiness::Readiness;
use praxis_utils_readiness::Token;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use super::timeout_warning::spawn_snapshot_timeout_warning;
use super::warnings::format_snapshot_warnings;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn run_ghost_snapshot_capture(
    session: Arc<Session>,
    ctx: Arc<TurnContext>,
    token: Token,
    cancellation_token: CancellationToken,
) {
    let warnings_enabled = !ctx.ghost_snapshot.disable_warnings;
    let snapshot_done = warnings_enabled.then(|| {
        spawn_snapshot_timeout_warning(
            Arc::clone(&session),
            Arc::clone(&ctx),
            cancellation_token.clone(),
        )
    });

    let ctx_for_task = Arc::clone(&ctx);
    let cancelled = tokio::select! {
        _ = cancellation_token.cancelled() => true,
        _ = async {
            capture_snapshot(Arc::clone(&session), Arc::clone(&ctx), ctx_for_task, warnings_enabled).await;
        } => false,
    };

    if let Some(done) = snapshot_done {
        let _ = done.send(());
    }

    if cancelled {
        info!("ghost snapshot task cancelled");
    }

    match ctx.tool_call_gate.mark_ready(token).await {
        Ok(true) => info!("ghost snapshot gate marked ready"),
        Ok(false) => warn!("ghost snapshot gate already ready"),
        Err(err) => warn!("failed to mark ghost snapshot ready: {err}"),
    }
}

async fn capture_snapshot(
    session: Arc<Session>,
    ctx: Arc<TurnContext>,
    ctx_for_task: Arc<TurnContext>,
    warnings_enabled: bool,
) {
    let repo_path = ctx_for_task.cwd.clone();
    let ghost_snapshot = ctx_for_task.ghost_snapshot.clone();
    let ghost_snapshot_for_commit = ghost_snapshot.clone();
    let capture_result = tokio::task::spawn_blocking(move || {
        let options =
            CreateGhostCommitOptions::new(&repo_path).ghost_snapshot(ghost_snapshot_for_commit);
        create_ghost_commit_with_report(&options)
    })
    .await;

    match capture_result {
        Ok(Ok((ghost_commit, report))) => {
            info!("ghost snapshot blocking task finished");
            if warnings_enabled {
                send_snapshot_report_warnings(&session, &ctx_for_task, &ghost_snapshot, &report)
                    .await;
            }
            session
                .record_conversation_items(
                    &ctx,
                    &[ResponseItem::GhostSnapshot {
                        ghost_commit: ghost_commit.clone(),
                    }],
                )
                .await;
            info!("ghost commit captured: {}", ghost_commit.id());
        }
        Ok(Err(err)) => match err {
            GitToolingError::NotAGitRepository { .. } => info!(
                sub_id = ctx_for_task.sub_id.as_str(),
                "skipping ghost snapshot because current directory is not a Git repository"
            ),
            _ => {
                warn!(
                    sub_id = ctx_for_task.sub_id.as_str(),
                    "failed to capture ghost snapshot: {err}"
                );
            }
        },
        Err(err) => {
            warn!(
                sub_id = ctx_for_task.sub_id.as_str(),
                "ghost snapshot task panicked: {err}"
            );
            let message = format!("Snapshots disabled after ghost snapshot panic: {err}.");
            session
                .notify_background_event(&ctx_for_task, message)
                .await;
        }
    }
}

async fn send_snapshot_report_warnings(
    session: &Session,
    ctx: &TurnContext,
    ghost_snapshot: &GhostSnapshotConfig,
    report: &GhostSnapshotReport,
) {
    for message in format_snapshot_warnings(
        ghost_snapshot.ignore_large_untracked_files,
        ghost_snapshot.ignore_large_untracked_dirs,
        report,
    ) {
        session
            .send_event(ctx, EventMsg::Warning(WarningEvent { message }))
            .await;
    }
}

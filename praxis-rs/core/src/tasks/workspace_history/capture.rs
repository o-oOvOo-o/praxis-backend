use std::sync::Arc;

use praxis_protocol::models::ResponseItem;
use praxis_protocol::workspace_history::WorkspaceCheckpointRef;
use praxis_utils_readiness::Readiness;
use praxis_utils_readiness::Token;
use praxis_workspace_history::CaptureCheckpointRequest;
use praxis_workspace_history::WorkspaceHistoryService;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use super::timeout_warning::spawn_snapshot_timeout_warning;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn run_workspace_checkpoint_capture(
    session: Arc<Session>,
    ctx: Arc<TurnContext>,
    token: Token,
    cancellation_token: CancellationToken,
) {
    let checkpoint_done = spawn_snapshot_timeout_warning(
        Arc::clone(&session),
        Arc::clone(&ctx),
        cancellation_token.clone(),
    );

    let cancelled = tokio::select! {
        _ = cancellation_token.cancelled() => true,
        result = capture_workspace_checkpoint(
            Arc::clone(&session),
            Arc::clone(&ctx),
            "turn-boundary".to_string(),
            true,
        ) => {
            if let Err(error) = result {
                warn!(sub_id = ctx.sub_id.as_str(), "failed to capture workspace checkpoint: {error}");
            }
            false
        },
    };

    let _ = checkpoint_done.send(());
    if cancelled {
        info!("workspace checkpoint task cancelled");
    }
    match ctx.tool_call_gate.mark_ready(token).await {
        Ok(true) => info!("workspace checkpoint gate marked ready"),
        Ok(false) => warn!("workspace checkpoint gate already ready"),
        Err(error) => warn!("failed to mark workspace checkpoint ready: {error}"),
    }
}

pub(crate) async fn capture_workspace_checkpoint(
    session: Arc<Session>,
    ctx: Arc<TurnContext>,
    operation_id: String,
    record_unchanged: bool,
) -> anyhow::Result<WorkspaceCheckpointRef> {
    let service = WorkspaceHistoryService::open(
        &ctx.config.praxis_home,
        ctx.workspace_history.clone(),
    )
    .await?;
    let checkpoint = service
        .capture(CaptureCheckpointRequest {
            workspace_root: ctx.cwd.to_path_buf(),
            thread_id: Some(session.conversation_id.to_string()),
            turn_id: Some(ctx.sub_id.clone()),
            operation_id: Some(operation_id),
        })
        .await?;
    if record_unchanged || checkpoint.changed_file_count > 0 {
        session
            .record_conversation_items(
                &ctx,
                &[ResponseItem::WorkspaceCheckpoint {
                    checkpoint: checkpoint.clone(),
                }],
            )
            .await;
    }
    let maintenance = service.clone();
    tokio::spawn(async move {
        if let Err(error) = maintenance.prune().await {
            warn!("workspace history maintenance failed: {error}");
        }
    });
    info!(checkpoint_id = %checkpoint.id, changed_files = checkpoint.changed_file_count, "workspace checkpoint captured");
    Ok(checkpoint)
}

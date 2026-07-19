use praxis_protocol::workspace_history::WorkspaceCheckpointRef;
use praxis_workspace_history::WorkspaceHistoryService;
use tracing::error;

use crate::praxis::TurnContext;

pub(super) enum RestoreWorkspaceCheckpointResult {
    Restored { checkpoint_id: String, short_id: String },
    Failed { message: String },
}

pub(super) async fn restore_workspace_checkpoint(
    ctx: &TurnContext,
    checkpoint: WorkspaceCheckpointRef,
) -> RestoreWorkspaceCheckpointResult {
    let checkpoint_id = checkpoint.id;
    let result = match WorkspaceHistoryService::open(
        &ctx.config.praxis_home,
        ctx.workspace_history.clone(),
    )
    .await
    {
        Ok(service) => service
            .restore(
                checkpoint_id,
                checkpoint.thread_id.clone(),
                checkpoint.turn_id.clone(),
            )
            .await,
        Err(error) => Err(error),
    };

    match result {
        Ok(_) => {
            let checkpoint_id = checkpoint_id.to_string();
            let short_id = checkpoint_id.chars().take(7).collect();
            RestoreWorkspaceCheckpointResult::Restored {
                checkpoint_id,
                short_id,
            }
        }
        Err(error) => {
            let message = format!("Failed to restore checkpoint {checkpoint_id}: {error}");
            error!("{message}");
            RestoreWorkspaceCheckpointResult::Failed { message }
        }
    }
}

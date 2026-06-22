use praxis_git_utils::GhostCommit;
use praxis_git_utils::RestoreGhostCommitOptions;
use praxis_git_utils::restore_ghost_commit_with_options;
use tracing::error;
use tracing::warn;

use crate::praxis::TurnContext;

pub(super) enum RestoreGhostSnapshotResult {
    Restored { commit_id: String, short_id: String },
    Failed { message: String },
}

pub(super) async fn restore_ghost_snapshot(
    ctx: &TurnContext,
    ghost_commit: GhostCommit,
) -> RestoreGhostSnapshotResult {
    let commit_id = ghost_commit.id().to_string();
    let repo_path = ctx.cwd.clone();
    let ghost_snapshot = ctx.ghost_snapshot.clone();
    let restore_result = tokio::task::spawn_blocking(move || {
        let options = RestoreGhostCommitOptions::new(&repo_path).ghost_snapshot(ghost_snapshot);
        restore_ghost_commit_with_options(&options, &ghost_commit)
    })
    .await;

    match restore_result {
        Ok(Ok(())) => {
            let short_id: String = commit_id.chars().take(7).collect();
            RestoreGhostSnapshotResult::Restored {
                commit_id,
                short_id,
            }
        }
        Ok(Err(err)) => {
            let message = format!("Failed to restore snapshot {commit_id}: {err}");
            warn!("{message}");
            RestoreGhostSnapshotResult::Failed { message }
        }
        Err(err) => {
            let message = format!("Failed to restore snapshot {commit_id}: {err}");
            error!("{message}");
            RestoreGhostSnapshotResult::Failed { message }
        }
    }
}

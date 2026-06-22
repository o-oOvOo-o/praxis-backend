use std::sync::Arc;
use std::time::Duration;

use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::WarningEvent;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;
use crate::praxis::TurnContext;

const SNAPSHOT_WARNING_THRESHOLD: Duration = Duration::from_secs(240);

pub(super) fn spawn_snapshot_timeout_warning(
    session: Arc<Session>,
    ctx: Arc<TurnContext>,
    cancellation_token: CancellationToken,
) -> oneshot::Sender<()> {
    let (snapshot_done_tx, snapshot_done_rx) = oneshot::channel::<()>();
    tokio::task::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(SNAPSHOT_WARNING_THRESHOLD) => {
                session
                    .send_event(
                        &ctx,
                        EventMsg::Warning(WarningEvent {
                            message: "Repository snapshot is taking longer than expected. Large untracked or ignored files can slow snapshots; consider adding large files or directories to .gitignore or disabling `undo` in your config.".to_string()
                        }),
                    )
                    .await;
            }
            _ = snapshot_done_rx => {}
            _ = cancellation_token.cancelled() => {}
        }
    });
    snapshot_done_tx
}

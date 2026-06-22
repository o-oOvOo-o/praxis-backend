use std::sync::Arc;

use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::events::send_undo_completed;
use super::events::send_undo_started;
use super::events::undo_completed;
use super::history::find_latest_ghost_snapshot;
use super::restore::RestoreGhostSnapshotResult;
use super::restore::restore_ghost_snapshot;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::AgentTaskKind;
use crate::tasks::AgentTask;

struct UndoTask;

impl UndoTask {
    fn new() -> Self {
        Self
    }
}

impl Session {
    pub(crate) async fn start_undo_task(self: &Arc<Self>, sub_id: String) {
        let turn_context = self.new_default_turn_with_sub_id(sub_id).await;
        self.spawn_task(turn_context, Vec::new(), UndoTask::new())
            .await;
    }
}

#[async_trait]
impl AgentTask for UndoTask {
    fn kind(&self) -> AgentTaskKind {
        AgentTaskKind::Undo
    }

    fn span_name(&self) -> &'static str {
        "agent_task.undo"
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        ctx: Arc<TurnContext>,
        _input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        let _ = session
            .services
            .session_telemetry
            .counter("praxis.task.undo", /*inc*/ 1, &[]);
        send_undo_started(&session, ctx.as_ref()).await;

        if cancellation_token.is_cancelled() {
            send_undo_completed(
                &session,
                ctx.as_ref(),
                undo_completed(false, Some("Undo cancelled.".to_string())),
            )
            .await;
            return None;
        }

        let history = session.clone_history().await;
        let mut items = history.raw_items().to_vec();
        let Some((idx, ghost_commit)) = find_latest_ghost_snapshot(&items) else {
            send_undo_completed(
                &session,
                ctx.as_ref(),
                undo_completed(
                    false,
                    Some("No ghost snapshot available to undo.".to_string()),
                ),
            )
            .await;
            return None;
        };

        match restore_ghost_snapshot(ctx.as_ref(), ghost_commit).await {
            RestoreGhostSnapshotResult::Restored {
                commit_id,
                short_id,
            } => {
                items.remove(idx);
                let reference_context_item = session.reference_context_item().await;
                session.replace_history(items, reference_context_item).await;
                info!(commit_id = commit_id, "Undo restored ghost snapshot");
                send_undo_completed(
                    &session,
                    ctx.as_ref(),
                    undo_completed(true, Some(format!("Undo restored snapshot {short_id}."))),
                )
                .await;
            }
            RestoreGhostSnapshotResult::Failed { message } => {
                send_undo_completed(&session, ctx.as_ref(), undo_completed(false, Some(message)))
                    .await;
            }
        }

        None
    }
}

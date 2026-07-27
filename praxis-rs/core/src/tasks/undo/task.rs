use std::sync::Arc;

use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::events::send_undo_completed;
use super::events::send_undo_started;
use super::events::undo_completed;
use super::history::find_latest_workspace_checkpoint;
use super::history::find_previous_workspace_checkpoint;
use super::restore::RestoreWorkspaceCheckpointResult;
use super::restore::restore_workspace_checkpoint;
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
        let Some((idx, latest_checkpoint)) = find_latest_workspace_checkpoint(&items) else {
            send_undo_completed(
                &session,
                ctx.as_ref(),
                undo_completed(
                    false,
                    Some("No workspace checkpoint available to undo.".to_string()),
                ),
            )
            .await;
            return None;
        };
        let checkpoint = match praxis_workspace_history::WorkspaceHistoryService::open(
            &ctx.config.praxis_home,
            ctx.workspace_history.clone(),
        )
        .await
        {
            Ok(service) => match service.manifest(latest_checkpoint.id).await {
                Ok(manifest)
                    if manifest
                        .operation_id
                        .as_deref()
                        .is_some_and(|operation| operation.starts_with("tool:")) =>
                {
                    find_previous_workspace_checkpoint(&items, idx)
                        .unwrap_or_else(|| latest_checkpoint.clone())
                }
                Ok(_) => latest_checkpoint.clone(),
                Err(error) => {
                    send_undo_completed(
                        &session,
                        ctx.as_ref(),
                        undo_completed(
                            false,
                            Some(format!("Failed to inspect workspace checkpoint: {error}")),
                        ),
                    )
                    .await;
                    return None;
                }
            },
            Err(error) => {
                send_undo_completed(
                    &session,
                    ctx.as_ref(),
                    undo_completed(
                        false,
                        Some(format!("Failed to open workspace history: {error}")),
                    ),
                )
                .await;
                return None;
            }
        };

        match restore_workspace_checkpoint(ctx.as_ref(), checkpoint).await {
            RestoreWorkspaceCheckpointResult::Restored {
                checkpoint_id,
                short_id,
            } => {
                items.remove(idx);
                let reference_context_item = session.reference_context_item().await;
                session.replace_history(items, reference_context_item).await;
                info!(
                    checkpoint_id = checkpoint_id,
                    "Undo restored workspace checkpoint"
                );
                send_undo_completed(
                    &session,
                    ctx.as_ref(),
                    undo_completed(true, Some(format!("Undo restored snapshot {short_id}."))),
                )
                .await;
            }
            RestoreWorkspaceCheckpointResult::Failed { message } => {
                send_undo_completed(&session, ctx.as_ref(), undo_completed(false, Some(message)))
                    .await;
            }
        }

        None
    }
}

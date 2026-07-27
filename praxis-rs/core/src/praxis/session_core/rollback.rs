mod completion;
mod replay_plan;
mod rollout_source;
mod validation;

use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::ThreadRolledBackEvent;
use praxis_protocol::protocol::WorkspaceRestoreEvent;
use praxis_protocol::workspace_history::WorkspaceCheckpointId;
use praxis_workspace_history::WorkspaceHistoryService;

use super::super::Session;

impl Session {
    pub(crate) async fn rollback_thread(
        &self,
        sub_id: String,
        num_turns: u32,
        restore_checkpoint: Option<WorkspaceCheckpointId>,
    ) {
        if validation::reject_invalid_request(self, &sub_id, num_turns).await {
            return;
        }

        let turn_context = self.new_default_turn_with_sub_id(sub_id).await;
        let Some(rollout_history) = rollout_source::load_flushed_history(self, &turn_context).await
        else {
            return;
        };

        let workspace_restore = if let Some(checkpoint_id) = restore_checkpoint {
            let service = match WorkspaceHistoryService::open(
                &turn_context.config.praxis_home,
                turn_context.workspace_history.clone(),
            )
            .await
            {
                Ok(service) => service,
                Err(error) => {
                    self.raw_event_emitter(&turn_context.sub_id)
                        .error(
                            format!("failed to open workspace history: {error}"),
                            Some(PraxisErrorInfo::ThreadRollbackFailed),
                        )
                        .await;
                    return;
                }
            };
            let manifest = match service.manifest(checkpoint_id).await {
                Ok(manifest) => manifest,
                Err(error) => {
                    self.raw_event_emitter(&turn_context.sub_id)
                        .error(
                            format!("failed to load workspace checkpoint {checkpoint_id}: {error}"),
                            Some(PraxisErrorInfo::ThreadRollbackFailed),
                        )
                        .await;
                    return;
                }
            };
            let thread_id = self.conversation_id.to_string();
            let expected_checkpoint = match service
                .checkpoint_for_rewind(thread_id.as_str(), num_turns)
                .await
            {
                Ok(Some(checkpoint)) => checkpoint,
                Ok(None) => {
                    self.raw_event_emitter(&turn_context.sub_id)
                        .error(
                            "no workspace checkpoint exists for this thread rewind",
                            Some(PraxisErrorInfo::ThreadRollbackFailed),
                        )
                        .await;
                    return;
                }
                Err(error) => {
                    self.raw_event_emitter(&turn_context.sub_id)
                        .error(
                            format!("failed to resolve workspace checkpoint: {error}"),
                            Some(PraxisErrorInfo::ThreadRollbackFailed),
                        )
                        .await;
                    return;
                }
            };
            if manifest.thread_id.as_deref() != Some(thread_id.as_str())
                || manifest.operation_id.as_deref() != Some("turn-boundary")
                || expected_checkpoint.id != checkpoint_id
            {
                self.raw_event_emitter(&turn_context.sub_id)
                    .error(
                        "workspace checkpoint does not belong to this thread rewind",
                        Some(PraxisErrorInfo::ThreadRollbackFailed),
                    )
                    .await;
                return;
            }
            match service
                .restore(
                    checkpoint_id,
                    Some(thread_id),
                    Some(turn_context.sub_id.clone()),
                )
                .await
            {
                Ok(outcome) => Some(WorkspaceRestoreEvent {
                    checkpoint_id,
                    restored_files: outcome.restored_files,
                    removed_files: outcome.removed_files,
                }),
                Err(error) => {
                    self.raw_event_emitter(&turn_context.sub_id)
                        .error(
                            format!(
                                "failed to restore workspace checkpoint {checkpoint_id}: {error}"
                            ),
                            Some(PraxisErrorInfo::ThreadRollbackFailed),
                        )
                        .await;
                    return;
                }
            }
        } else {
            None
        };

        let rollback_event = ThreadRolledBackEvent {
            num_turns,
            workspace_restore,
        };
        let rollback_msg = replay_plan::rollback_message(rollback_event);
        let replay_items = replay_plan::build_items(rollout_history, rollback_msg.clone());
        completion::commit(self, turn_context.as_ref(), rollback_msg, replay_items).await;
    }
}

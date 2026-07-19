use super::*;
use praxis_workspace_history::CaptureCheckpointRequest;
use praxis_workspace_history::WorkspaceHistoryService;

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn thread_rewind_preview(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadRewindPreviewParams,
    ) {
        if params.num_turns == 0 {
            self.send_invalid_request_error(request_id, "numTurns must be >= 1".to_string())
                .await;
            return;
        }
        let Some((thread_id, thread)) = self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };
        let turn_is_active = {
            let state = self.thread_state_manager.thread_state(thread_id).await;
            let state = state.lock().await;
            state.active_turn_snapshot().is_some()
        };
        if turn_is_active {
            self.send_invalid_request_error(
                request_id,
                "cannot edit a previous message while the thread is running".to_string(),
            )
            .await;
            return;
        }
        let service = match WorkspaceHistoryService::open(
            &self.config.praxis_home,
            self.config.workspace_history.clone(),
        )
        .await
        {
            Ok(service) => service,
            Err(error) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to open workspace history: {error}"),
                )
                .await;
                return;
            }
        };
        let target = match service
            .checkpoint_for_rewind(&thread_id.to_string(), params.num_turns)
            .await
        {
            Ok(target) => target,
            Err(error) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to resolve rewind checkpoint: {error}"),
                )
                .await;
                return;
            }
        };
        let Some(target) = target else {
            self.outgoing
                .send_response(
                    request_id,
                    ThreadRewindPreviewResponse {
                        checkpoint_id: None,
                        changed_files: Vec::new(),
                    },
                )
                .await;
            return;
        };
        let config_snapshot = thread.config_snapshot().await;
        let current = match service
            .capture(CaptureCheckpointRequest {
                workspace_root: config_snapshot.cwd,
                thread_id: Some(thread_id.to_string()),
                turn_id: None,
                operation_id: Some("rewind-preview".to_string()),
            })
            .await
        {
            Ok(current) => current,
            Err(error) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to capture rewind preview: {error}"),
                )
                .await;
                return;
            }
        };
        match service.summarize_changes(target.id, current.id).await {
            Ok(changed_files) => {
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadRewindPreviewResponse {
                            checkpoint_id: Some(target.id),
                            changed_files,
                        },
                    )
                    .await;
            }
            Err(error) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to summarize rewind workspace changes: {error}"),
                )
                .await;
            }
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_increment_elicitation(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadIncrementElicitationParams,
    ) {
        let Some((_, thread)) = self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };

        match thread.increment_out_of_band_elicitation_count().await {
            Ok(count) => {
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadIncrementElicitationResponse {
                            count,
                            paused: count > 0,
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to increment out-of-band elicitation counter: {err}"),
                )
                .await;
            }
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_decrement_elicitation(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadDecrementElicitationParams,
    ) {
        let Some((_, thread)) = self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };

        match thread.decrement_out_of_band_elicitation_count().await {
            Ok(count) => {
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadDecrementElicitationResponse {
                            count,
                            paused: count > 0,
                        },
                    )
                    .await;
            }
            Err(PraxisErr::InvalidRequest(message)) => {
                self.send_invalid_request_error(request_id, message).await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to decrement out-of-band elicitation counter: {err}"),
                )
                .await;
            }
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_rollback(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadRollbackParams,
    ) {
        let ThreadRollbackParams {
            thread_id,
            num_turns,
            workspace_action,
        } = params;
        let restore_checkpoint = match workspace_action {
            ThreadRewindWorkspaceAction::Keep => None,
            ThreadRewindWorkspaceAction::Restore { checkpoint_id } => Some(checkpoint_id),
        };

        if num_turns == 0 {
            self.send_invalid_request_error(request_id, "numTurns must be >= 1".to_string())
                .await;
            return;
        }

        let Some((thread_id, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        let rollback_already_in_progress = {
            let thread_state = self.thread_state_manager.thread_state(thread_id).await;
            let mut thread_state = thread_state.lock().await;
            if thread_state.pending_rollbacks.is_some() {
                true
            } else {
                thread_state.pending_rollbacks = Some(request_id.clone());
                false
            }
        };
        if rollback_already_in_progress {
            self.send_invalid_request_error(
                request_id,
                "rollback already in progress for this thread".to_string(),
            )
            .await;
            return;
        }

        if let Err(err) = self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                Op::ThreadRollback {
                    num_turns,
                    restore_checkpoint,
                },
            )
            .await
        {
            // No ThreadRollback event will arrive if an error occurs.
            // Clean up and reply immediately.
            let thread_state = self.thread_state_manager.thread_state(thread_id).await;
            let mut thread_state = thread_state.lock().await;
            thread_state.pending_rollbacks = None;
            drop(thread_state);

            self.send_internal_error(request_id, format!("failed to start rollback: {err}"))
                .await;
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_compact_start(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadCompactStartParams,
    ) {
        let ThreadCompactStartParams { thread_id } = params;

        let Some((_, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        match self
            .submit_core_op(&request_id, thread.as_ref(), Op::Compact)
            .await
        {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadCompactStartResponse {})
                    .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to start compaction: {err}"))
                    .await;
            }
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_background_terminals_clean(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadBackgroundTerminalsCleanParams,
    ) {
        let ThreadBackgroundTerminalsCleanParams { thread_id } = params;

        let Some((_, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        match self
            .submit_core_op(&request_id, thread.as_ref(), Op::CleanBackgroundTerminals)
            .await
        {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadBackgroundTerminalsCleanResponse {})
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to clean background terminals: {err}"),
                )
                .await;
            }
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_shell_command(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadShellCommandParams,
    ) {
        let ThreadShellCommandParams { thread_id, command } = params;
        let command = command.trim().to_string();
        if command.is_empty() {
            self.outgoing
                .send_error(
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "command must not be empty".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let Some((_, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        match self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                Op::RunUserShellCommand { command },
            )
            .await
        {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadShellCommandResponse {})
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to start shell command: {err}"),
                )
                .await;
            }
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_history_append(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadHistoryAppendParams,
    ) {
        let ThreadHistoryAppendParams { thread_id, text } = params;
        let Some((_, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        match self
            .submit_core_op(&request_id, thread.as_ref(), Op::AddToHistory { text })
            .await
        {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadHistoryAppendResponse {})
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to append thread history: {err}"),
                )
                .await;
            }
        }
    }

    pub(in crate::praxis_message_processor) async fn thread_history_entry_get(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadHistoryEntryGetParams,
    ) {
        let ThreadHistoryEntryGetParams {
            thread_id,
            offset,
            log_id,
        } = params;
        let Some((_, thread)) = self
            .ensure_thread_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        match self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                Op::GetHistoryEntryRequest { offset, log_id },
            )
            .await
        {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, ThreadHistoryEntryGetResponse {})
                    .await;
            }
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to request thread history entry: {err}"),
                )
                .await;
            }
        }
    }
}

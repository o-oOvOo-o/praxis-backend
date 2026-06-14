use super::thread_lifecycle_api::ThreadShutdownResult;
use super::*;
use crate::json_rpc_error::internal_error;
use crate::json_rpc_error::invalid_request;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_archive(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadArchiveParams,
    ) {
        let thread_id = match self.parse_thread_id(&params.thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let rollout_path =
            match find_thread_rollout_path(&self.config, thread_id, ThreadRolloutScope::Active)
                .await
            {
                Ok(path) => path,
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            };

        let thread_id_str = thread_id.to_string();
        match self.archive_thread_common(thread_id, &rollout_path).await {
            Ok(()) => {
                let response = ThreadArchiveResponse {};
                self.outgoing.send_response(request_id, response).await;
                let notification = ThreadArchivedNotification {
                    thread_id: thread_id_str,
                };
                self.outgoing
                    .send_server_notification(ServerNotification::ThreadArchived(notification))
                    .await;
            }
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
            }
        }
    }

    pub(crate) async fn thread_delete(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadDeleteParams,
    ) {
        let thread_id = match self.parse_thread_id(&params.thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let rollout_path = match find_thread_rollout_path(
            &self.config,
            thread_id,
            ThreadRolloutScope::Any,
        )
        .await
        {
            Ok(path) => path,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let thread_id_str = thread_id.to_string();
        match self.delete_thread_common(thread_id, &rollout_path).await {
            Ok(()) => {
                self.outgoing
                    .send_response(request_id, ThreadDeleteResponse {})
                    .await;
                self.outgoing
                    .send_server_notification(ServerNotification::ThreadClosed(
                        ThreadClosedNotification {
                            thread_id: thread_id_str,
                        },
                    ))
                    .await;
            }
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
            }
        }
    }

    pub(crate) async fn thread_unarchive(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadUnarchiveParams,
    ) {
        let thread_id = match self.parse_thread_id(&params.thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let archived_path =
            match find_thread_rollout_path(&self.config, thread_id, ThreadRolloutScope::Archived)
                .await
            {
                Ok(path) => path,
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            };

        let rollout_path_display = archived_path.display().to_string();
        let fallback_provider = self.config.model_provider_id.clone();
        let state_db_ctx = get_state_db(&self.config).await;
        let archived_folder = self
            .config
            .praxis_home
            .join(praxis_core::ARCHIVED_SESSIONS_SUBDIR);

        let result: Result<Thread, JSONRPCErrorError> = async {
            let canonical_archived_dir = tokio::fs::canonicalize(&archived_folder).await.map_err(
                |err| {
                    internal_error(format!(
                        "failed to unarchive thread: unable to resolve archived directory: {err}"
                    ))
                },
            )?;
            let canonical_rollout_path = tokio::fs::canonicalize(&archived_path).await;
            let canonical_rollout_path = if let Ok(path) = canonical_rollout_path
                && path.starts_with(&canonical_archived_dir)
            {
                path
            } else {
                return Err(invalid_request(format!(
                    "rollout path `{rollout_path_display}` must be in archived directory"
                )));
            };

            let required_suffix = format!("{thread_id}.jsonl");
            let Some(file_name) = canonical_rollout_path.file_name().map(OsStr::to_owned) else {
                return Err(invalid_request(format!(
                    "rollout path `{rollout_path_display}` missing file name"
                )));
            };
            if !file_name
                .to_string_lossy()
                .ends_with(required_suffix.as_str())
            {
                return Err(invalid_request(format!(
                    "rollout path `{rollout_path_display}` does not match thread id {thread_id}"
                )));
            }

            let Some((year, month, day)) = rollout_date_parts(&file_name) else {
                return Err(invalid_request(format!(
                    "rollout path `{rollout_path_display}` missing filename timestamp"
                )));
            };

            let sessions_folder = self.config.praxis_home.join(praxis_core::SESSIONS_SUBDIR);
            let dest_dir = sessions_folder.join(year).join(month).join(day);
            let restored_path = dest_dir.join(&file_name);
            tokio::fs::create_dir_all(&dest_dir)
                .await
                .map_err(|err| internal_error(format!("failed to unarchive thread: {err}")))?;
            tokio::fs::rename(&canonical_rollout_path, &restored_path)
                .await
                .map_err(|err| internal_error(format!("failed to unarchive thread: {err}")))?;
            tokio::task::spawn_blocking({
                let restored_path = restored_path.clone();
                move || -> std::io::Result<()> {
                    let times = FileTimes::new().set_modified(SystemTime::now());
                    OpenOptions::new()
                        .append(true)
                        .open(&restored_path)?
                        .set_times(times)?;
                    Ok(())
                }
            })
            .await
            .map_err(|err| {
                internal_error(format!(
                    "failed to update unarchived thread timestamp: {err}"
                ))
            })?
            .map_err(|err| {
                internal_error(format!(
                    "failed to update unarchived thread timestamp: {err}"
                ))
            })?;
            if let Some(ctx) = state_db_ctx {
                let _ = ctx
                    .mark_unarchived(thread_id, restored_path.as_path())
                    .await;
            }
            let summary = hydrate_rollout_summary_with_state_db(
                &self.config,
                read_summary_from_rollout(restored_path.as_path(), fallback_provider.as_str())
                    .await
                    .map_err(|err| {
                        internal_error(format!("failed to read unarchived thread: {err}"))
                    })?,
            )
            .await;
            Ok(summary_to_thread(summary))
        }
        .await;

        match result {
            Ok(mut thread) => {
                self.apply_thread_runtime_state(
                    &mut thread,
                    /*has_live_in_progress_turn*/ false,
                )
                .await;
                self.attach_thread_name(thread_id, &mut thread).await;
                let thread_id = thread.id.clone();
                let response = ThreadUnarchiveResponse { thread };
                self.outgoing.send_response(request_id, response).await;
                let notification = ThreadUnarchivedNotification { thread_id };
                self.outgoing
                    .send_server_notification(ServerNotification::ThreadUnarchived(notification))
                    .await;
            }
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
            }
        }
    }
    async fn archive_thread_common(
        &mut self,
        thread_id: ThreadId,
        rollout_path: &Path,
    ) -> Result<(), JSONRPCErrorError> {
        // Verify rollout_path is under sessions dir.
        let rollout_folder = self.config.praxis_home.join(praxis_core::SESSIONS_SUBDIR);

        let canonical_sessions_dir = match tokio::fs::canonicalize(&rollout_folder).await {
            Ok(path) => path,
            Err(err) => {
                return Err(internal_error(format!(
                    "failed to archive thread: unable to resolve sessions directory: {err}"
                )));
            }
        };
        let canonical_rollout_path = tokio::fs::canonicalize(rollout_path).await;
        let canonical_rollout_path = if let Ok(path) = canonical_rollout_path
            && path.starts_with(&canonical_sessions_dir)
        {
            path
        } else {
            return Err(invalid_request(format!(
                "rollout path `{}` must be in sessions directory",
                rollout_path.display()
            )));
        };

        // Verify file name matches thread id.
        let required_suffix = format!("{thread_id}.jsonl");
        let Some(file_name) = canonical_rollout_path.file_name().map(OsStr::to_owned) else {
            return Err(invalid_request(format!(
                "rollout path `{}` missing file name",
                rollout_path.display()
            )));
        };
        if !file_name
            .to_string_lossy()
            .ends_with(required_suffix.as_str())
        {
            return Err(invalid_request(format!(
                "rollout path `{}` does not match thread id {thread_id}",
                rollout_path.display()
            )));
        }

        let mut state_db_ctx = None;

        // If the thread is active, request shutdown and wait briefly.
        let removed_conversation = self.thread_manager.remove_thread(&thread_id).await;
        if let Some(conversation) = removed_conversation {
            if let Some(ctx) = conversation.state_db() {
                state_db_ctx = Some(ctx);
            }
            info!("thread {thread_id} was active; shutting down");
            match Self::wait_for_thread_shutdown(&conversation).await {
                ThreadShutdownResult::Complete => {}
                ThreadShutdownResult::SubmitFailed => {
                    error!(
                        "failed to submit Shutdown to thread {thread_id}; proceeding with archive"
                    );
                }
                ThreadShutdownResult::TimedOut => {
                    warn!("thread {thread_id} shutdown timed out; proceeding with archive");
                }
            }
        }
        self.finalize_thread_teardown(thread_id).await;

        if state_db_ctx.is_none() {
            state_db_ctx = get_state_db(&self.config).await;
        }

        // Move the rollout file to archived.
        let praxis_home = self.config.praxis_home.clone();
        let result: std::io::Result<()> = async move {
            let archive_folder = praxis_home.join(praxis_core::ARCHIVED_SESSIONS_SUBDIR);
            tokio::fs::create_dir_all(&archive_folder).await?;
            let archived_path = archive_folder.join(&file_name);
            tokio::fs::rename(&canonical_rollout_path, &archived_path).await?;
            if let Some(ctx) = state_db_ctx {
                let _ = ctx
                    .mark_archived(thread_id, archived_path.as_path(), Utc::now())
                    .await;
            }
            Ok(())
        }
        .await;

        let result =
            result.map_err(|err| internal_error(format!("failed to archive thread: {err}")));
        result
    }

    async fn delete_thread_common(
        &mut self,
        thread_id: ThreadId,
        rollout_path: &Path,
    ) -> Result<(), JSONRPCErrorError> {
        let rollout_folder = self.config.praxis_home.join(praxis_core::SESSIONS_SUBDIR);
        let archived_folder = self
            .config
            .praxis_home
            .join(praxis_core::ARCHIVED_SESSIONS_SUBDIR);
        let canonical_sessions_dir = tokio::fs::canonicalize(&rollout_folder).await.ok();
        let canonical_archived_dir = tokio::fs::canonicalize(&archived_folder).await.ok();
        let canonical_rollout_path =
            tokio::fs::canonicalize(rollout_path).await.map_err(|err| {
                invalid_request(format!(
                    "failed to delete thread: unable to resolve rollout path `{}`: {err}",
                    rollout_path.display()
                ))
            })?;

        let allowed_path = canonical_sessions_dir
            .as_ref()
            .is_some_and(|dir| canonical_rollout_path.starts_with(dir))
            || canonical_archived_dir
                .as_ref()
                .is_some_and(|dir| canonical_rollout_path.starts_with(dir));
        if !allowed_path {
            return Err(invalid_request(format!(
                "rollout path `{}` must be in sessions or archived directory",
                rollout_path.display()
            )));
        }

        let required_suffix = format!("{thread_id}.jsonl");
        let Some(file_name) = canonical_rollout_path.file_name().map(OsStr::to_owned) else {
            return Err(invalid_request(format!(
                "rollout path `{}` missing file name",
                rollout_path.display()
            )));
        };
        if !file_name
            .to_string_lossy()
            .ends_with(required_suffix.as_str())
        {
            return Err(invalid_request(format!(
                "rollout path `{}` does not match thread id {thread_id}",
                rollout_path.display()
            )));
        }

        let mut state_db_ctx = None;
        let removed_conversation = self.thread_manager.remove_thread(&thread_id).await;
        if let Some(conversation) = removed_conversation {
            if let Some(ctx) = conversation.state_db() {
                state_db_ctx = Some(ctx);
            }
            info!("thread {thread_id} was active; shutting down before delete");
            match Self::wait_for_thread_shutdown(&conversation).await {
                ThreadShutdownResult::Complete => {}
                ThreadShutdownResult::SubmitFailed => {
                    error!(
                        "failed to submit Shutdown to thread {thread_id}; proceeding with delete"
                    );
                }
                ThreadShutdownResult::TimedOut => {
                    warn!("thread {thread_id} shutdown timed out; proceeding with delete");
                }
            }
        }
        self.finalize_thread_teardown(thread_id).await;

        if state_db_ctx.is_none() {
            state_db_ctx = get_state_db(&self.config).await;
        }

        tokio::fs::remove_file(&canonical_rollout_path)
            .await
            .map_err(|err| internal_error(format!("failed to delete thread: {err}")))?;
        if let Some(ctx) = state_db_ctx {
            let _ = ctx.delete_thread(thread_id).await;
        }
        Ok(())
    }
}

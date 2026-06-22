use super::thread_projection_api::load_thread_summary_from_state_db_context;
use super::thread_store_api::ThreadStore;
use super::*;
use crate::json_rpc_error::internal_error;
use crate::json_rpc_error::invalid_request;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_set_name(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadSetNameParams,
    ) {
        let ThreadSetNameParams { thread_id, name } = params;
        let Some(thread_id) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };
        let Some(name) = praxis_core::util::normalize_thread_name(&name) else {
            self.send_invalid_request_error(
                request_id,
                "thread name must not be empty".to_string(),
            )
            .await;
            return;
        };

        if let Ok(thread) = self.thread_manager.get_thread(thread_id).await {
            if let Err(err) = self
                .submit_core_op(
                    &request_id,
                    thread.as_ref(),
                    Op::SetThreadName { name: name.clone() },
                )
                .await
            {
                self.send_internal_error(request_id, format!("failed to set thread name: {err}"))
                    .await;
                return;
            }

            self.outgoing
                .send_response(request_id, ThreadSetNameResponse {})
                .await;
            return;
        }

        let thread_store = ThreadStore::new(&self.config);
        let thread_exists = match thread_store.thread_exists(thread_id, None).await {
            Ok(exists) => exists,
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to locate thread id {thread_id}: {err}"),
                )
                .await;
                return;
            }
        };

        if !thread_exists {
            self.send_invalid_request_error(request_id, format!("thread not found: {thread_id}"))
                .await;
            return;
        }

        if let Err(err) = thread_store.write_thread_name(thread_id, &name).await {
            self.send_internal_error(request_id, format!("failed to set thread name: {err}"))
                .await;
            return;
        }

        self.outgoing
            .send_response(request_id, ThreadSetNameResponse {})
            .await;
        let notification = ThreadNameUpdatedNotification {
            thread_id: thread_id.to_string(),
            thread_name: Some(name),
        };
        self.outgoing
            .send_server_notification(ServerNotification::ThreadNameUpdated(notification))
            .await;
    }

    pub(crate) async fn thread_regenerate_name(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadRegenerateNameParams,
    ) {
        let ThreadRegenerateNameParams { thread_id } = params;
        let Some(thread_id) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        let thread = match self.thread_manager.get_thread(thread_id).await {
            Ok(thread) => thread,
            Err(_) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("thread must be loaded to regenerate its name: {thread_id}"),
                )
                .await;
                return;
            }
        };

        let thread_name = match thread.regenerate_thread_name().await {
            Ok(thread_name) => thread_name,
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to regenerate thread name: {err}"),
                )
                .await;
                return;
            }
        };

        self.outgoing
            .send_response(request_id, ThreadRegenerateNameResponse { thread_name })
            .await;
    }

    pub(crate) async fn thread_metadata_update(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadMetadataUpdateParams,
    ) {
        let ThreadMetadataUpdateParams {
            thread_id,
            git_info,
            selfwork_plan_path,
        } = params;

        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        if git_info.is_none() && selfwork_plan_path.is_none() {
            self.send_invalid_request_error(
                request_id,
                "thread metadata update must include at least one field".to_string(),
            )
            .await;
            return;
        }

        let loaded_thread = self.thread_manager.get_thread(thread_uuid).await.ok();
        let mut state_db_ctx = loaded_thread.as_ref().and_then(|thread| thread.state_db());
        if state_db_ctx.is_none() {
            state_db_ctx = get_state_db(&self.config).await;
        }
        let Some(state_db_ctx) = state_db_ctx else {
            self.send_internal_error(
                request_id,
                format!("sqlite state db unavailable for thread {thread_uuid}"),
            )
            .await;
            return;
        };

        if let Err(error) = self
            .ensure_thread_metadata_row_exists(thread_uuid, &state_db_ctx, loaded_thread.as_ref())
            .await
        {
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        if let Some(ThreadMetadataGitInfoUpdateParams {
            sha,
            branch,
            origin_url,
        }) = git_info
        {
            if sha.is_none() && branch.is_none() && origin_url.is_none() {
                self.send_invalid_request_error(
                    request_id,
                    "gitInfo must include at least one field".to_string(),
                )
                .await;
                return;
            }

            let git_sha = match sha {
                Some(Some(sha)) => {
                    let sha = sha.trim().to_string();
                    if sha.is_empty() {
                        self.send_invalid_request_error(
                            request_id,
                            "gitInfo.sha must not be empty".to_string(),
                        )
                        .await;
                        return;
                    }
                    Some(Some(sha))
                }
                Some(None) => Some(None),
                None => None,
            };
            let git_branch = match branch {
                Some(Some(branch)) => {
                    let branch = branch.trim().to_string();
                    if branch.is_empty() {
                        self.send_invalid_request_error(
                            request_id,
                            "gitInfo.branch must not be empty".to_string(),
                        )
                        .await;
                        return;
                    }
                    Some(Some(branch))
                }
                Some(None) => Some(None),
                None => None,
            };
            let git_origin_url = match origin_url {
                Some(Some(origin_url)) => {
                    let origin_url = origin_url.trim().to_string();
                    if origin_url.is_empty() {
                        self.send_invalid_request_error(
                            request_id,
                            "gitInfo.originUrl must not be empty".to_string(),
                        )
                        .await;
                        return;
                    }
                    Some(Some(origin_url))
                }
                Some(None) => Some(None),
                None => None,
            };

            let updated = match state_db_ctx
                .update_thread_git_info(
                    thread_uuid,
                    git_sha.as_ref().map(|value| value.as_deref()),
                    git_branch.as_ref().map(|value| value.as_deref()),
                    git_origin_url.as_ref().map(|value| value.as_deref()),
                )
                .await
            {
                Ok(updated) => updated,
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!("failed to update thread metadata for {thread_uuid}: {err}"),
                    )
                    .await;
                    return;
                }
            };
            if !updated {
                self.send_internal_error(
                    request_id,
                    format!("thread metadata disappeared before update completed: {thread_uuid}"),
                )
                .await;
                return;
            }
        }

        if let Some(selfwork_plan_path) = selfwork_plan_path {
            let updated = match selfwork_plan_path {
                Some(path) => {
                    if path.as_os_str().is_empty() {
                        self.send_invalid_request_error(
                            request_id,
                            "selfworkPlanPath must not be empty".to_string(),
                        )
                        .await;
                        return;
                    }
                    state_db_ctx
                        .update_thread_selfwork_plan_path(thread_uuid, Some(path.as_path()))
                        .await
                }
                None => {
                    state_db_ctx
                        .update_thread_selfwork_plan_path(thread_uuid, None)
                        .await
                }
            };
            match updated {
                Ok(true) => {}
                Ok(false) => {
                    self.send_internal_error(
                        request_id,
                        format!(
                            "thread metadata disappeared before update completed: {thread_uuid}"
                        ),
                    )
                    .await;
                    return;
                }
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!("failed to update thread metadata for {thread_uuid}: {err}"),
                    )
                    .await;
                    return;
                }
            }
        }

        let Some(mut thread) =
            load_thread_summary_from_state_db_context(Some(&state_db_ctx), thread_uuid).await
        else {
            self.send_internal_error(
                request_id,
                format!("failed to reload updated thread metadata for {thread_uuid}"),
            )
            .await;
            return;
        };

        self.attach_thread_name(thread_uuid, &mut thread).await;
        self.project_thread_runtime_state(&mut thread, /*has_live_in_progress_turn*/ false)
            .await;

        self.outgoing
            .send_response(request_id, ThreadMetadataUpdateResponse { thread })
            .await;
    }

    async fn ensure_thread_metadata_row_exists(
        &self,
        thread_uuid: ThreadId,
        state_db_ctx: &Arc<StateRuntime>,
        loaded_thread: Option<&Arc<PraxisThread>>,
    ) -> Result<(), JSONRPCErrorError> {
        match state_db_ctx.get_thread(thread_uuid).await {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(err) => {
                return Err(internal_error(format!(
                    "failed to load thread metadata for {thread_uuid}: {err}"
                )));
            }
        }

        if let Some(thread) = loaded_thread {
            let Some(rollout_path) = thread.rollout_path() else {
                return Err(invalid_request(format!(
                    "ephemeral thread does not support metadata updates: {thread_uuid}"
                )));
            };

            reconcile_rollout(
                Some(state_db_ctx),
                rollout_path.as_path(),
                self.config.model_provider_id.as_str(),
                /*builder*/ None,
                &[],
                /*archived_only*/ None,
                /*new_thread_memory_mode*/ None,
            )
            .await;

            match state_db_ctx.get_thread(thread_uuid).await {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {}
                Err(err) => {
                    return Err(internal_error(format!(
                        "failed to load reconciled thread metadata for {thread_uuid}: {err}"
                    )));
                }
            }

            let config_snapshot = thread.config_snapshot().await;
            let model_provider = config_snapshot.model_provider_id.clone();
            let mut builder = ThreadMetadataBuilder::new(
                thread_uuid,
                rollout_path,
                Utc::now(),
                config_snapshot.session_source.clone(),
            );
            builder.model_provider = Some(model_provider.clone());
            builder.cwd = config_snapshot.cwd.clone();
            builder.cli_version = Some(env!("CARGO_PKG_VERSION").to_string());
            builder.sandbox_policy = config_snapshot.sandbox_policy.clone();
            builder.approval_mode = config_snapshot.approval_policy;
            let metadata = builder.build(model_provider.as_str());
            if let Err(err) = state_db_ctx.insert_thread_if_absent(&metadata).await {
                return Err(internal_error(format!(
                    "failed to create thread metadata for {thread_uuid}: {err}"
                )));
            }
            return Ok(());
        }

        let rollout_path = find_thread_rollout_path_or_not_found(&self.config, thread_uuid).await?;

        reconcile_rollout(
            Some(state_db_ctx),
            rollout_path.as_path(),
            self.config.model_provider_id.as_str(),
            /*builder*/ None,
            &[],
            /*archived_only*/ None,
            /*new_thread_memory_mode*/ None,
        )
        .await;

        match state_db_ctx.get_thread(thread_uuid).await {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(internal_error(format!(
                "failed to create thread metadata from rollout for {thread_uuid}"
            ))),
            Err(err) => Err(internal_error(format!(
                "failed to load reconciled thread metadata for {thread_uuid}: {err}"
            ))),
        }
    }
    pub(crate) async fn attach_thread_name(&self, thread_id: ThreadId, thread: &mut Thread) {
        thread.name = self.thread_name(thread_id).await;
    }

    async fn thread_name(&self, thread_id: ThreadId) -> Option<String> {
        ThreadStore::new(&self.config)
            .resolve_thread_name(thread_id)
            .await
    }
}

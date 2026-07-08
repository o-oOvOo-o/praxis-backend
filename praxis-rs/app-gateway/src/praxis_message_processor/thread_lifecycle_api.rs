use super::thread_runtime_api::project_thread_runtime_state_from_watch;
use super::thread_store_api::ThreadHistorySource;
use super::thread_store_api::ThreadStore;
use super::thread_store_api::ThreadTurnHydration;
use super::*;
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;

mod connection;
mod teardown;
mod thread_fork;
mod thread_list;
mod thread_ops;
mod thread_read;
mod thread_start;
mod unsubscribe;

pub(super) use teardown::ThreadShutdownResult;

fn project_scope_root(path: &Path) -> PathBuf {
    praxis_git_utils::resolve_root_git_project_for_trust(path).unwrap_or_else(|| path.to_path_buf())
}

impl PraxisMessageProcessor {
    pub(super) async fn thread_resume(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadResumeParams,
    ) {
        if let Ok(thread_id) = ThreadId::from_string(&params.thread_id)
            && self
                .pending_thread_unloads
                .lock()
                .await
                .contains(&thread_id)
        {
            self.send_invalid_request_error(
                request_id,
                format!(
                    "thread {thread_id} is closing; retry thread/resume after the thread is closed"
                ),
            )
            .await;
            return;
        }

        if self
            .resume_running_thread(request_id.clone(), &params)
            .await
        {
            return;
        }

        let ThreadResumeParams {
            thread_id,
            history,
            path,
            model,
            model_provider,
            service_tier,
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox,
            config: mut request_overrides,
            base_instructions,
            developer_instructions,
            personality,
            dynamic_tools,
            persist_extended_history,
            turn_limit,
        } = params;

        let thread_history = if let Some(history) = history {
            let Some(thread_history) = self
                .resume_thread_from_history(request_id.clone(), history.as_slice())
                .await
            else {
                return;
            };
            thread_history
        } else {
            let Some(thread_history) = self
                .resume_thread_from_rollout(request_id.clone(), &thread_id, path.as_ref())
                .await
            else {
                return;
            };
            thread_history
        };

        let history_cwd = thread_history.session_cwd();
        let mut typesafe_overrides = self.build_thread_config_overrides(
            model,
            model_provider,
            service_tier,
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox,
            base_instructions,
            developer_instructions,
            personality,
        );
        let persisted_resume_metadata = self
            .load_and_apply_persisted_resume_metadata(
                &thread_history,
                &mut request_overrides,
                &mut typesafe_overrides,
            )
            .await;

        // Derive a Config using the same logic as new conversation, honoring overrides if provided.
        let cloud_requirements = self.current_cloud_requirements();
        let cli_overrides = self.current_cli_overrides();
        let runtime_feature_enablement = self.current_runtime_feature_enablement();
        let config = match derive_config_for_cwd(
            &cli_overrides,
            request_overrides,
            typesafe_overrides,
            history_cwd,
            &cloud_requirements,
            &self.config.praxis_home,
            &runtime_feature_enablement,
        )
        .await
        {
            Ok(config) => config,
            Err(err) => {
                let error = config_load_error(&err);
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let fallback_model_provider = config.model_provider_id.clone();
        let response_history = thread_history.clone();
        let core_dynamic_tools = match build_core_dynamic_tools(dynamic_tools) {
            Ok(tools) => tools,
            Err(message) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message,
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let core_dynamic_tool_count = core_dynamic_tools.len();

        match self
            .thread_manager
            .resume_thread_with_history_and_dynamic_tools(
                config,
                thread_history,
                self.auth_manager.clone(),
                core_dynamic_tools,
                persist_extended_history,
                self.request_trace_context(&request_id).await,
            )
            .instrument(tracing::info_span!(
                "app_gateway.thread_resume.create_thread",
                otel.name = "app_gateway.thread_resume.create_thread",
                thread_resume.dynamic_tool_count = core_dynamic_tool_count,
                thread_resume.persist_extended_history = persist_extended_history,
            ))
            .await
        {
            Ok(ThreadSpawnResult {
                thread_id,
                thread,
                session_configured,
            }) => {
                let SessionConfiguredEvent { rollout_path, .. } = session_configured;
                let Some(rollout_path) = rollout_path else {
                    self.send_internal_error(
                        request_id,
                        format!("rollout path missing for thread {thread_id}"),
                    )
                    .await;
                    return;
                };
                // Auto-attach a thread listener when resuming a thread.
                Self::log_listener_attach_result(
                    self.ensure_conversation_listener(
                        thread_id,
                        request_id.connection_id,
                        /*raw_events_enabled*/ false,
                    )
                    .await,
                    thread_id,
                    request_id.connection_id,
                    "thread",
                );

                let mut thread = match self
                    .load_thread_from_resume_source_or_send_internal(
                        thread_id,
                        thread.as_ref(),
                        &response_history,
                        rollout_path.as_path(),
                        fallback_model_provider.as_str(),
                        persisted_resume_metadata.as_ref(),
                        turn_limit,
                    )
                    .await
                {
                    Ok(thread) => thread,
                    Err(message) => {
                        self.send_internal_error(request_id, message).await;
                        return;
                    }
                };

                self.thread_watch_manager
                    .upsert_thread(thread.clone())
                    .await;

                self.project_thread_runtime_state_with_turn_cleanup(
                    &mut thread,
                    /*has_live_in_progress_turn*/ false,
                )
                .await;

                let response = ThreadResumeResponse {
                    thread,
                    model: session_configured.model,
                    model_provider: session_configured.model_provider_id,
                    service_tier: session_configured.service_tier,
                    cwd: session_configured.cwd,
                    approval_policy: session_configured.approval_policy.into(),
                    approvals_reviewer: session_configured.approvals_reviewer.into(),
                    sandbox: session_configured.sandbox_policy.into(),
                    reasoning_effort: session_configured.reasoning_effort,
                    history_log_id: session_configured.history_log_id,
                    history_entry_count: u64::try_from(session_configured.history_entry_count)
                        .unwrap_or(u64::MAX),
                };
                if self.config.features.enabled(Feature::GeneralAnalytics) {
                    self.analytics_events_client.track_thread_initialized(
                        request_id.connection_id.0,
                        thread_initialized_fact(
                            &response.thread,
                            &response.model,
                            ThreadInitializationMode::Resumed,
                        ),
                    );
                }

                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("error resuming thread: {err:#}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn load_and_apply_persisted_resume_metadata(
        &self,
        thread_history: &InitialHistory,
        request_overrides: &mut Option<HashMap<String, serde_json::Value>>,
        typesafe_overrides: &mut ConfigOverrides,
    ) -> Option<ThreadMetadata> {
        let InitialHistory::Resumed(resumed_history) = thread_history else {
            return None;
        };
        let state_db_ctx = get_state_db(&self.config).await?;
        let persisted_metadata = state_db_ctx
            .get_thread(resumed_history.conversation_id)
            .await
            .ok()
            .flatten()?;
        merge_persisted_resume_metadata(request_overrides, typesafe_overrides, &persisted_metadata);
        Some(persisted_metadata)
    }

    fn running_resume_context_override(params: &ThreadResumeParams) -> Option<Op> {
        let has_live_overrides = params.cwd.is_some()
            || params.approval_policy.is_some()
            || params.approvals_reviewer.is_some()
            || params.sandbox.is_some()
            || params.model_provider.is_some()
            || params.model.is_some()
            || params.service_tier.is_some()
            || params.personality.is_some();
        if !has_live_overrides {
            return None;
        }

        Some(Op::OverrideTurnContext {
            cwd: params.cwd.as_ref().map(PathBuf::from),
            approval_policy: params
                .approval_policy
                .map(praxis_app_gateway_protocol::AskForApproval::to_core),
            approvals_reviewer: params
                .approvals_reviewer
                .map(praxis_app_gateway_protocol::ApprovalsReviewer::to_core),
            sandbox_policy: params.sandbox.map(|mode| {
                praxis_core::config::sandbox_projection::sandbox_policy_from_mode(mode.to_core())
            }),
            windows_sandbox_level: None,
            model_provider: params.model_provider.clone(),
            model: params.model.clone(),
            effort: None,
            summary: None,
            service_tier: params.service_tier,
            collaboration_mode: None,
            personality: params.personality,
        })
    }

    async fn apply_running_resume_overrides_or_send_error(
        &self,
        request_id: &ConnectionRequestId,
        thread_id: ThreadId,
        thread: &PraxisThread,
        params: &ThreadResumeParams,
    ) -> Option<ThreadConfigSnapshot> {
        let Some(override_op) = Self::running_resume_context_override(params) else {
            return Some(thread.config_snapshot().await);
        };

        let before = thread.config_snapshot().await;
        let before_mismatches = collect_resume_override_mismatches(params, &before);
        if before_mismatches.is_empty() {
            return Some(before);
        }

        tracing::info!(
            thread_id = %thread_id,
            mismatches = %before_mismatches.join("; "),
            "applying thread/resume overrides to running thread"
        );
        if let Err(err) = self.submit_core_op(request_id, thread, override_op).await {
            self.send_internal_error(
                request_id.clone(),
                format!("failed to apply running thread resume overrides: {err}"),
            )
            .await;
            return None;
        }

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = thread.config_snapshot().await;
            let mismatches = collect_resume_override_mismatches(params, &snapshot);
            if mismatches.is_empty() {
                tracing::info!(
                    thread_id = %thread_id,
                    approval_policy = ?snapshot.approval_policy,
                    sandbox_policy = ?snapshot.sandbox_policy,
                    "thread/resume overrides applied to running thread"
                );
                return Some(snapshot);
            }
            if tokio::time::Instant::now() >= deadline {
                self.send_invalid_request_error(
                    request_id.clone(),
                    format!(
                        "thread/resume overrides did not become effective for running thread {thread_id}: {}",
                        mismatches.join("; ")
                    ),
                )
                .await;
                return None;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn resume_running_thread(
        &mut self,
        request_id: ConnectionRequestId,
        params: &ThreadResumeParams,
    ) -> bool {
        if let Ok(existing_thread_id) = ThreadId::from_string(&params.thread_id)
            && let Ok(existing_thread) = self.thread_manager.get_thread(existing_thread_id).await
        {
            if params.history.is_some() {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "cannot resume thread {existing_thread_id} with history while it is already running"
                    ),
                )
                .await;
                return true;
            }

            if params
                .dynamic_tools
                .as_ref()
                .is_some_and(|tools| !tools.is_empty())
            {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "cannot change dynamic tools for running thread {existing_thread_id}; close the thread before resuming it with dynamic tools"
                    ),
                )
                .await;
                return true;
            }

            let rollout_path = match resolve_thread_rollout_path(
                &self.config,
                existing_thread_id,
                existing_thread.rollout_path(),
            )
            .await
            {
                Ok(path) => path,
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return true;
                }
            };

            if let Some(requested_path) = params.path.as_ref()
                && requested_path != &rollout_path
            {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "cannot resume running thread {existing_thread_id} with mismatched path: requested `{}`, active `{}`",
                        requested_path.display(),
                        rollout_path.display()
                    ),
                )
                .await;
                return true;
            }

            let thread_state = self
                .thread_state_manager
                .thread_state(existing_thread_id)
                .await;
            self.ensure_listener_task_running(
                existing_thread_id,
                existing_thread.clone(),
                thread_state.clone(),
            )
            .await;

            let Some(config_snapshot) = self
                .apply_running_resume_overrides_or_send_error(
                    &request_id,
                    existing_thread_id,
                    existing_thread.as_ref(),
                    params,
                )
                .await
            else {
                return true;
            };
            let thread_summary = match load_thread_summary_for_rollout(
                &self.config,
                existing_thread_id,
                rollout_path.as_path(),
                config_snapshot.model_provider_id.as_str(),
                /*persisted_metadata*/ None,
            )
            .await
            {
                Ok(thread) => thread,
                Err(message) => {
                    self.send_internal_error(request_id, message).await;
                    return true;
                }
            };

            let listener_command_tx = {
                let thread_state = thread_state.lock().await;
                thread_state.listener_command_tx()
            };
            let Some(listener_command_tx) = listener_command_tx else {
                let err = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!(
                        "failed to enqueue running thread resume for thread {existing_thread_id}: thread listener is not running"
                    ),
                    data: None,
                };
                self.outgoing.send_error(request_id, err).await;
                return true;
            };

            let command = crate::thread_state::ThreadListenerCommand::SendThreadResumeResponse(
                Box::new(crate::thread_state::PendingThreadResumeRequest {
                    request_id: request_id.clone(),
                    rollout_path: rollout_path.clone(),
                    config_snapshot,
                    thread_summary,
                    turn_limit: params.turn_limit,
                }),
            );
            if listener_command_tx.send(command).await.is_err() {
                let err = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!(
                        "failed to enqueue running thread resume for thread {existing_thread_id}: thread listener command channel is closed"
                    ),
                    data: None,
                };
                self.outgoing.send_error(request_id, err).await;
            }
            return true;
        }
        false
    }

    async fn resume_thread_from_history(
        &self,
        request_id: ConnectionRequestId,
        history: &[ResponseItem],
    ) -> Option<InitialHistory> {
        if history.is_empty() {
            self.send_invalid_request_error(request_id, "history must not be empty".to_string())
                .await;
            return None;
        }
        Some(InitialHistory::Forked(
            history
                .iter()
                .cloned()
                .map(RolloutItem::ResponseItem)
                .collect(),
        ))
    }

    async fn resume_thread_from_rollout(
        &self,
        request_id: ConnectionRequestId,
        thread_id: &str,
        path: Option<&PathBuf>,
    ) -> Option<InitialHistory> {
        let rollout_path = if let Some(path) = path {
            path.clone()
        } else {
            let Some((_, rollout_path)) = self
                .ensure_thread_rollout_for_request(thread_id, ThreadRolloutScope::Any, &request_id)
                .await
            else {
                return None;
            };
            rollout_path
        };

        match ThreadStore::read_initial_history(&rollout_path).await {
            Ok(initial_history) => Some(initial_history),
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to load rollout `{}`: {err}", rollout_path.display()),
                )
                .await;
                None
            }
        }
    }

    async fn load_thread_from_resume_source_or_send_internal(
        &self,
        thread_id: ThreadId,
        thread: &PraxisThread,
        thread_history: &InitialHistory,
        rollout_path: &Path,
        fallback_provider: &str,
        persisted_resume_metadata: Option<&ThreadMetadata>,
        turn_limit: Option<u32>,
    ) -> std::result::Result<Thread, String> {
        let thread = match thread_history {
            InitialHistory::Resumed(resumed) => {
                load_thread_summary_for_rollout(
                    &self.config,
                    resumed.conversation_id,
                    resumed.rollout_path.as_path(),
                    fallback_provider,
                    persisted_resume_metadata,
                )
                .await
            }
            InitialHistory::Forked(items) => {
                let config_snapshot = thread.config_snapshot().await;
                let mut thread = build_thread_from_snapshot(
                    thread_id,
                    &config_snapshot,
                    Some(rollout_path.into()),
                );
                thread.preview = ThreadStore::preview_from_rollout_items(items);
                Ok(thread)
            }
            InitialHistory::New => Err(format!(
                "failed to build resume response for thread {thread_id}: initial history missing"
            )),
        };
        let mut thread = thread?;
        thread.id = thread_id.to_string();
        thread.path = Some(rollout_path.to_path_buf());
        let history_items = thread_history.get_rollout_items();
        ThreadStore::hydrate_turns(
            &mut thread,
            ThreadHistorySource::RolloutItems(&history_items),
            ThreadTurnHydration::recent(turn_limit.map(|limit| limit as usize)),
            /*active_turn*/ None,
        )
        .await?;
        self.attach_thread_name(thread_id, &mut thread).await;
        Ok(thread)
    }
}

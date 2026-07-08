use super::*;

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn thread_fork(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadForkParams,
    ) {
        let ThreadForkParams {
            thread_id,
            path,
            model,
            model_provider,
            service_tier,
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox,
            config: cli_overrides,
            base_instructions,
            developer_instructions,
            ephemeral,
            persist_extended_history,
        } = params;

        let (rollout_path, source_thread_id) = if let Some(path) = path {
            (path, None)
        } else {
            let Some((existing_thread_id, path)) = self
                .ensure_thread_rollout_for_request(&thread_id, ThreadRolloutScope::Any, &request_id)
                .await
            else {
                return;
            };
            (path, Some(existing_thread_id))
        };

        let history_cwd = ThreadStore::new(&self.config)
            .read_history_cwd(source_thread_id, rollout_path.as_path())
            .await;

        // Persist Windows sandbox mode.
        let mut cli_overrides = cli_overrides.unwrap_or_default();
        if cfg!(windows) {
            match WindowsSandboxLevel::from_config(&self.config) {
                WindowsSandboxLevel::Elevated => {
                    cli_overrides
                        .insert("windows.sandbox".to_string(), serde_json::json!("elevated"));
                }
                WindowsSandboxLevel::RestrictedToken => {
                    cli_overrides.insert(
                        "windows.sandbox".to_string(),
                        serde_json::json!("unelevated"),
                    );
                }
                WindowsSandboxLevel::Disabled => {}
            }
        }
        let request_overrides = if cli_overrides.is_empty() {
            None
        } else {
            Some(cli_overrides)
        };
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
            /*personality*/ None,
        );
        typesafe_overrides.ephemeral = ephemeral.then_some(true);
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
                self.outgoing
                    .send_error(request_id, config_load_error(&err))
                    .await;
                return;
            }
        };

        let fallback_model_provider = config.model_provider_id.clone();

        let ThreadSpawnResult {
            thread_id,
            thread: forked_thread,
            session_configured,
            ..
        } = match self
            .thread_manager
            .fork_thread(
                ThreadForkSnapshot::Interrupted,
                config,
                rollout_path.clone(),
                persist_extended_history,
                self.request_trace_context(&request_id).await,
            )
            .await
        {
            Ok(thread) => thread,
            Err(err) => {
                match err {
                    PraxisErr::Io(_) | PraxisErr::Json(_) => {
                        self.send_invalid_request_error(
                            request_id,
                            format!("failed to load rollout `{}`: {err}", rollout_path.display()),
                        )
                        .await;
                    }
                    PraxisErr::InvalidRequest(message) => {
                        self.send_invalid_request_error(request_id, message).await;
                    }
                    _ => {
                        self.send_internal_error(
                            request_id,
                            format!("error forking thread: {err}"),
                        )
                        .await;
                    }
                }
                return;
            }
        };

        // Auto-attach a conversation listener when forking a thread.
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

        // Persistent forks materialize their own rollout immediately. Ephemeral forks stay
        // pathless, so they rebuild their visible history from the copied source rollout instead.
        let mut thread = if let Some(fork_rollout_path) = session_configured.rollout_path.as_ref() {
            match load_thread_summary_for_rollout(
                &self.config,
                thread_id,
                fork_rollout_path.as_path(),
                fallback_model_provider.as_str(),
                None,
            )
            .await
            {
                Ok(thread) => thread,
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!("failed to load forked thread {thread_id}: {err}"),
                    )
                    .await;
                    return;
                }
            }
        } else {
            let config_snapshot = forked_thread.config_snapshot().await;
            // forked thread names do not inherit the source thread name
            let mut thread =
                build_thread_from_snapshot(thread_id, &config_snapshot, /*path*/ None);
            let source_rollout_path = rollout_path.as_path();
            let history_items = match ThreadStore::read_rollout_items(source_rollout_path).await {
                Ok(items) => items,
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!(
                            "failed to load source rollout `{}` for thread {thread_id}: {err}",
                            rollout_path.display()
                        ),
                    )
                    .await;
                    return;
                }
            };
            thread.preview = ThreadStore::preview_from_rollout_items(&history_items);
            if let Err(message) = ThreadStore::hydrate_turns(
                &mut thread,
                ThreadHistorySource::RolloutItems(&history_items),
                ThreadTurnHydration::all(),
                /*active_turn*/ None,
            )
            .await
            {
                self.send_internal_error(request_id, message).await;
                return;
            }
            thread
        };

        if let Some(fork_rollout_path) = session_configured.rollout_path.as_ref()
            && let Err(message) = ThreadStore::hydrate_turns(
                &mut thread,
                ThreadHistorySource::RolloutPath(fork_rollout_path.as_path()),
                ThreadTurnHydration::all(),
                /*active_turn*/ None,
            )
            .await
        {
            self.send_internal_error(request_id, message).await;
            return;
        }
        thread.name = session_configured.thread_name.clone();

        self.thread_watch_manager
            .upsert_thread_silently(thread.clone())
            .await;

        self.project_thread_runtime_state(&mut thread, /*has_live_in_progress_turn*/ false)
            .await;

        let response = ThreadForkResponse {
            thread: thread.clone(),
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
                    ThreadInitializationMode::Forked,
                ),
            );
        }

        self.outgoing.send_response(request_id, response).await;

        let notif = ThreadStartedNotification { thread };
        self.outgoing
            .send_server_notification(ServerNotification::ThreadStarted(notif))
            .await;
    }
}

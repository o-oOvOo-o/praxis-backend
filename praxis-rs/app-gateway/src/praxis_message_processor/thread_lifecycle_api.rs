use super::thread_listener_api::set_thread_status_and_interrupt_stale_turns;
use super::thread_store_api::ThreadHistorySource;
use super::thread_store_api::hydrate_thread_turns;
use super::thread_store_api::read_thread_turns_from_rollout;
use super::*;
use praxis_app_gateway_protocol::THREAD_LIST_DEFAULT_LIMIT;
use praxis_app_gateway_protocol::THREAD_LIST_MAX_LIMIT;
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;

struct ThreadListFilters {
    model_providers: Option<Vec<String>>,
    source_kinds: Option<Vec<ApiThreadSourceKind>>,
    archived: bool,
    cwd: Option<PathBuf>,
    search_term: Option<String>,
}

fn map_thread_source_kind(kind: ApiThreadSourceKind) -> praxis_rollout::ThreadSourceKind {
    match kind {
        ApiThreadSourceKind::Cli => praxis_rollout::ThreadSourceKind::Cli,
        ApiThreadSourceKind::VsCode => praxis_rollout::ThreadSourceKind::VsCode,
        ApiThreadSourceKind::Exec => praxis_rollout::ThreadSourceKind::Exec,
        ApiThreadSourceKind::AppGateway => praxis_rollout::ThreadSourceKind::AppGateway,
        ApiThreadSourceKind::SubAgent => praxis_rollout::ThreadSourceKind::SubAgent,
        ApiThreadSourceKind::SubAgentReview => praxis_rollout::ThreadSourceKind::SubAgentReview,
        ApiThreadSourceKind::SubAgentCompact => praxis_rollout::ThreadSourceKind::SubAgentCompact,
        ApiThreadSourceKind::SubAgentThreadSpawn => {
            praxis_rollout::ThreadSourceKind::SubAgentThreadSpawn
        }
        ApiThreadSourceKind::SubAgentOther => praxis_rollout::ThreadSourceKind::SubAgentOther,
        ApiThreadSourceKind::Unknown => praxis_rollout::ThreadSourceKind::Unknown,
    }
}

pub(super) enum ThreadShutdownResult {
    Complete,
    SubmitFailed,
    TimedOut,
}

impl PraxisMessageProcessor {
    pub(super) async fn thread_start(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadStartParams,
        request_context: RequestContext,
    ) {
        let ThreadStartParams {
            model,
            model_provider,
            service_tier,
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox,
            config,
            service_name,
            base_instructions,
            developer_instructions,
            dynamic_tools,
            mock_experimental_field: _mock_experimental_field,
            experimental_raw_events,
            personality,
            ephemeral,
            persist_extended_history,
        } = params;
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
        typesafe_overrides.ephemeral = ephemeral;
        let cloud_requirements = self.current_cloud_requirements();
        let cli_overrides = self.current_cli_overrides();
        let listener_task_context = ListenerTaskContext {
            thread_manager: Arc::clone(&self.thread_manager),
            thread_state_manager: self.thread_state_manager.clone(),
            outgoing: Arc::clone(&self.outgoing),
            analytics_events_client: self.analytics_events_client.clone(),
            general_analytics_enabled: self.config.features.enabled(Feature::GeneralAnalytics),
            thread_watch_manager: self.thread_watch_manager.clone(),
            fallback_model_provider: self.config.model_provider_id.clone(),
            praxis_home: self.config.praxis_home.clone(),
        };
        let request_trace = request_context.request_trace();
        let runtime_feature_enablement = self.current_runtime_feature_enablement();
        let thread_start_task = async move {
            Self::thread_start_task(
                listener_task_context,
                cli_overrides,
                runtime_feature_enablement,
                cloud_requirements,
                request_id,
                config,
                typesafe_overrides,
                dynamic_tools,
                persist_extended_history,
                service_name,
                experimental_raw_events,
                request_trace,
            )
            .await;
        };
        self.background_tasks
            .spawn(thread_start_task.instrument(request_context.span()));
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn thread_start_task(
        listener_task_context: ListenerTaskContext,
        cli_overrides: Vec<(String, TomlValue)>,
        runtime_feature_enablement: BTreeMap<String, bool>,
        cloud_requirements: CloudConfigBundleLoader,
        request_id: ConnectionRequestId,
        config_overrides: Option<HashMap<String, serde_json::Value>>,
        typesafe_overrides: ConfigOverrides,
        dynamic_tools: Option<Vec<ApiDynamicToolSpec>>,
        persist_extended_history: bool,
        service_name: Option<String>,
        experimental_raw_events: bool,
        request_trace: Option<W3cTraceContext>,
    ) {
        let config = match derive_config_from_params(
            &cli_overrides,
            config_overrides,
            typesafe_overrides,
            &cloud_requirements,
            &listener_task_context.praxis_home,
            &runtime_feature_enablement,
        )
        .await
        {
            Ok(config) => config,
            Err(err) => {
                let error = config_load_error(&err);
                listener_task_context
                    .outgoing
                    .send_error(request_id, error)
                    .await;
                return;
            }
        };

        let dynamic_tools = dynamic_tools.unwrap_or_default();
        let core_dynamic_tools = if dynamic_tools.is_empty() {
            Vec::new()
        } else {
            if let Err(message) = validate_dynamic_tools(&dynamic_tools) {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message,
                    data: None,
                };
                listener_task_context
                    .outgoing
                    .send_error(request_id, error)
                    .await;
                return;
            }
            dynamic_tools
                .into_iter()
                .map(|tool| CoreDynamicToolSpec {
                    name: tool.name,
                    description: tool.description,
                    input_schema: tool.input_schema,
                    defer_loading: tool.defer_loading,
                })
                .collect()
        };
        let core_dynamic_tool_count = core_dynamic_tools.len();

        match listener_task_context
            .thread_manager
            .start_thread_with_tools_and_service_name(
                config,
                core_dynamic_tools,
                persist_extended_history,
                service_name,
                request_trace,
            )
            .instrument(tracing::info_span!(
                "app_gateway.thread_start.create_thread",
                otel.name = "app_gateway.thread_start.create_thread",
                thread_start.dynamic_tool_count = core_dynamic_tool_count,
                thread_start.persist_extended_history = persist_extended_history,
            ))
            .await
        {
            Ok(new_conv) => {
                let ThreadSpawnResult {
                    thread_id,
                    thread,
                    session_configured,
                    ..
                } = new_conv;
                let config_snapshot = thread
                    .config_snapshot()
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.config_snapshot",
                        otel.name = "app_gateway.thread_start.config_snapshot",
                    ))
                    .await;
                let mut thread = build_thread_from_snapshot(
                    thread_id,
                    &config_snapshot,
                    session_configured.rollout_path.clone(),
                );

                // Auto-attach a thread listener when starting a thread.
                Self::log_listener_attach_result(
                    Self::ensure_conversation_listener_task(
                        listener_task_context.clone(),
                        thread_id,
                        request_id.connection_id,
                        experimental_raw_events,
                    )
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.attach_listener",
                        otel.name = "app_gateway.thread_start.attach_listener",
                        thread_start.experimental_raw_events = experimental_raw_events,
                    ))
                    .await,
                    thread_id,
                    request_id.connection_id,
                    "thread",
                );

                listener_task_context
                    .thread_watch_manager
                    .upsert_thread_silently(thread.clone())
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.upsert_thread",
                        otel.name = "app_gateway.thread_start.upsert_thread",
                    ))
                    .await;

                let loaded_status = listener_task_context
                    .thread_watch_manager
                    .loaded_status_for_thread(&thread.id)
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.resolve_status",
                        otel.name = "app_gateway.thread_start.resolve_status",
                    ))
                    .await;
                let control_state = listener_task_context
                    .thread_watch_manager
                    .loaded_control_state_for_thread(&thread.id)
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.resolve_status",
                        otel.name = "app_gateway.thread_start.resolve_status",
                    ))
                    .await;
                thread.status = resolve_thread_status(
                    loaded_status,
                    /*has_in_progress_turn*/ false,
                    control_state.as_ref(),
                );
                thread.control_state = control_state;

                let response = ThreadStartResponse {
                    thread: thread.clone(),
                    model: config_snapshot.model,
                    model_provider: config_snapshot.model_provider_id,
                    service_tier: config_snapshot.service_tier,
                    cwd: config_snapshot.cwd,
                    approval_policy: config_snapshot.approval_policy.into(),
                    approvals_reviewer: config_snapshot.approvals_reviewer.into(),
                    sandbox: config_snapshot.sandbox_policy.into(),
                    reasoning_effort: config_snapshot.reasoning_effort,
                };
                if listener_task_context.general_analytics_enabled {
                    listener_task_context
                        .analytics_events_client
                        .track_thread_initialized(
                            request_id.connection_id.0,
                            thread_initialized_fact(
                                &response.thread,
                                &response.model,
                                ThreadInitializationMode::New,
                            ),
                        );
                }

                listener_task_context
                    .outgoing
                    .send_response(request_id, response)
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.send_response",
                        otel.name = "app_gateway.thread_start.send_response",
                    ))
                    .await;

                let notif = ThreadStartedNotification { thread };
                listener_task_context
                    .outgoing
                    .send_server_notification(ServerNotification::ThreadStarted(notif))
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.notify_started",
                        otel.name = "app_gateway.thread_start.notify_started",
                    ))
                    .await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("error creating thread: {err}"),
                    data: None,
                };
                listener_task_context
                    .outgoing
                    .send_error(request_id, error)
                    .await;
            }
        }
    }

    pub(super) async fn thread_increment_elicitation(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadIncrementElicitationParams,
    ) {
        let (_, thread) = match self.load_thread(&params.thread_id).await {
            Ok(value) => value,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
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

    pub(super) async fn thread_decrement_elicitation(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadDecrementElicitationParams,
    ) {
        let (_, thread) = match self.load_thread(&params.thread_id).await {
            Ok(value) => value,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
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

    pub(super) async fn thread_rollback(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadRollbackParams,
    ) {
        let ThreadRollbackParams {
            thread_id,
            num_turns,
        } = params;

        if num_turns == 0 {
            self.send_invalid_request_error(request_id, "numTurns must be >= 1".to_string())
                .await;
            return;
        }

        let (thread_id, thread) = match self.load_thread(&thread_id).await {
            Ok(v) => v,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let request = request_id.clone();

        let rollback_already_in_progress = {
            let thread_state = self.thread_state_manager.thread_state(thread_id).await;
            let mut thread_state = thread_state.lock().await;
            if thread_state.pending_rollbacks.is_some() {
                true
            } else {
                thread_state.pending_rollbacks = Some(request.clone());
                false
            }
        };
        if rollback_already_in_progress {
            self.send_invalid_request_error(
                request.clone(),
                "rollback already in progress for this thread".to_string(),
            )
            .await;
            return;
        }

        if let Err(err) = self
            .submit_core_op(
                &request_id,
                thread.as_ref(),
                Op::ThreadRollback { num_turns },
            )
            .await
        {
            // No ThreadRollback event will arrive if an error occurs.
            // Clean up and reply immediately.
            let thread_state = self.thread_state_manager.thread_state(thread_id).await;
            let mut thread_state = thread_state.lock().await;
            thread_state.pending_rollbacks = None;
            drop(thread_state);

            self.send_internal_error(request, format!("failed to start rollback: {err}"))
                .await;
        }
    }

    pub(super) async fn thread_compact_start(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadCompactStartParams,
    ) {
        let ThreadCompactStartParams { thread_id } = params;

        let (_, thread) = match self.load_thread(&thread_id).await {
            Ok(v) => v,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
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

    pub(super) async fn thread_background_terminals_clean(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadBackgroundTerminalsCleanParams,
    ) {
        let ThreadBackgroundTerminalsCleanParams { thread_id } = params;

        let (_, thread) = match self.load_thread(&thread_id).await {
            Ok(v) => v,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
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

    pub(super) async fn thread_shell_command(
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

        let (_, thread) = match self.load_thread(&thread_id).await {
            Ok(v) => v,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
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

    pub(super) async fn thread_list(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadListParams,
    ) {
        let ThreadListParams {
            cursor,
            limit,
            sort_key,
            model_providers,
            source_kinds,
            archived,
            cwd,
            search_term,
        } = params;

        let requested_page_size = limit
            .unwrap_or(THREAD_LIST_DEFAULT_LIMIT)
            .clamp(1, THREAD_LIST_MAX_LIMIT) as usize;
        let core_sort_key = match sort_key.unwrap_or(ThreadSortKey::CreatedAt) {
            ThreadSortKey::CreatedAt => CoreThreadSortKey::CreatedAt,
            ThreadSortKey::UpdatedAt => CoreThreadSortKey::UpdatedAt,
        };
        let (summaries, next_cursor) = match self
            .list_threads_common(
                requested_page_size,
                cursor,
                core_sort_key,
                ThreadListFilters {
                    model_providers,
                    source_kinds,
                    archived: archived.unwrap_or(false),
                    cwd: cwd.map(PathBuf::from),
                    search_term,
                },
            )
            .await
        {
            Ok(r) => r,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let mut threads = Vec::with_capacity(summaries.len());
        let mut status_ids = Vec::with_capacity(summaries.len());

        for summary in summaries {
            let thread_name = summary.thread_name.clone();
            let mut thread = summary_to_thread(thread_summary_to_rollout_summary(summary));
            thread.name = thread_name;
            status_ids.push(thread.id.clone());
            threads.push(thread);
        }

        let statuses = self
            .thread_watch_manager
            .loaded_statuses_for_threads(status_ids.clone())
            .await;
        let control_states = self
            .thread_watch_manager
            .loaded_control_states_for_threads(status_ids)
            .await;

        let data = threads
            .into_iter()
            .map(|mut thread| {
                let control_state = control_states.get(&thread.id).cloned();
                if let Some(status) = statuses.get(&thread.id) {
                    thread.status = resolve_thread_status(
                        status.clone(),
                        /*has_in_progress_turn*/ false,
                        control_state.as_ref(),
                    );
                }
                thread.control_state = control_state;
                thread
            })
            .collect();
        let response = ThreadListResponse { data, next_cursor };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn thread_loaded_list(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadLoadedListParams,
    ) {
        let ThreadLoadedListParams { cursor, limit } = params;
        let mut data = self
            .thread_manager
            .list_thread_ids()
            .await
            .into_iter()
            .map(|thread_id| thread_id.to_string())
            .collect::<Vec<_>>();

        if data.is_empty() {
            let response = ThreadLoadedListResponse {
                data,
                next_cursor: None,
            };
            self.outgoing.send_response(request_id, response).await;
            return;
        }

        data.sort();
        let total = data.len();
        let start = match cursor {
            Some(cursor) => {
                let cursor = match ThreadId::from_string(&cursor) {
                    Ok(id) => id.to_string(),
                    Err(_) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("invalid cursor: {cursor}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return;
                    }
                };
                match data.binary_search(&cursor) {
                    Ok(idx) => idx + 1,
                    Err(idx) => idx,
                }
            }
            None => 0,
        };

        let effective_limit = limit
            .unwrap_or(THREAD_LIST_DEFAULT_LIMIT)
            .clamp(1, THREAD_LIST_MAX_LIMIT) as usize;
        let end = start.saturating_add(effective_limit).min(total);
        let page = data[start..end].to_vec();
        let next_cursor = page.last().filter(|_| end < total).cloned();

        let response = ThreadLoadedListResponse {
            data: page,
            next_cursor,
        };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn thread_read(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadReadParams,
    ) {
        let ThreadReadParams {
            thread_id,
            include_turns,
        } = params;

        let thread_uuid = match self.parse_thread_id(&thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let loaded_thread = self.thread_manager.get_thread(thread_uuid).await.ok();
        let directory = praxis_rollout::ThreadDirectory::open(&self.config).await;
        let directory_summary = match directory
            .read_thread_summary(thread_uuid, None, self.config.model_provider_id.as_str())
            .await
        {
            Ok(summary) => summary,
            Err(err) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to read thread {thread_uuid}: {err}"),
                )
                .await;
                return;
            }
        };
        let mut rollout_path = directory_summary
            .as_ref()
            .map(|summary| summary.path.clone());

        let mut thread = if let Some(summary) = directory_summary {
            let thread_name = summary.thread_name.clone();
            let mut thread = summary_to_thread(thread_summary_to_rollout_summary(summary));
            thread.name = thread_name;
            thread
        } else {
            let Some(thread) = loaded_thread.as_ref() else {
                self.send_invalid_request_error(
                    request_id,
                    format!("thread not loaded: {thread_uuid}"),
                )
                .await;
                return;
            };
            let config_snapshot = thread.config_snapshot().await;
            let loaded_rollout_path = thread.rollout_path();
            if include_turns && loaded_rollout_path.is_none() {
                self.send_invalid_request_error(
                    request_id,
                    "ephemeral threads do not support includeTurns".to_string(),
                )
                .await;
                return;
            }
            if include_turns {
                rollout_path = loaded_rollout_path.clone();
            }
            build_thread_from_snapshot(thread_uuid, &config_snapshot, loaded_rollout_path)
        };
        if thread.name.is_none() {
            self.attach_thread_name(thread_uuid, &mut thread).await;
        }

        if include_turns && let Some(rollout_path) = rollout_path.as_ref() {
            match read_thread_turns_from_rollout(rollout_path).await {
                Ok(turns) => {
                    thread.turns = turns;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    self.send_invalid_request_error(
                        request_id,
                        format!(
                            "thread {thread_uuid} is not materialized yet; includeTurns is unavailable before first user message"
                        ),
                    )
                    .await;
                    return;
                }
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!(
                            "failed to load rollout `{}` for thread {thread_uuid}: {err}",
                            rollout_path.display()
                        ),
                    )
                    .await;
                    return;
                }
            }
        }

        let has_live_in_progress_turn = if let Some(loaded_thread) = loaded_thread.as_ref() {
            matches!(loaded_thread.agent_status().await, AgentStatus::Running)
        } else {
            false
        };

        self.apply_thread_runtime_state(&mut thread, has_live_in_progress_turn)
            .await;
        let thread_status = thread.status.clone();
        let control_state = thread.control_state.clone();
        set_thread_status_and_interrupt_stale_turns(
            &mut thread,
            thread_status,
            has_live_in_progress_turn,
            control_state.as_ref(),
        );
        let response = ThreadReadResponse { thread };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(crate) fn thread_created_receiver(&self) -> broadcast::Receiver<ThreadId> {
        self.thread_manager.subscribe_thread_created()
    }

    pub(crate) async fn connection_initialized(&self, connection_id: ConnectionId) {
        self.thread_state_manager
            .connection_initialized(connection_id)
            .await;
    }

    pub(crate) async fn connection_closed(&mut self, connection_id: ConnectionId) {
        self.command_exec_manager
            .connection_closed(connection_id)
            .await;
        self.thread_state_manager
            .remove_connection(connection_id)
            .await;
    }

    pub(crate) fn subscribe_running_assistant_turn_count(&self) -> watch::Receiver<usize> {
        self.thread_watch_manager.subscribe_running_turn_count()
    }

    /// Best-effort: ensure initialized connections are subscribed to this thread.
    pub(crate) async fn try_attach_thread_listener(
        &mut self,
        thread_id: ThreadId,
        connection_ids: Vec<ConnectionId>,
    ) {
        if let Ok(thread) = self.thread_manager.get_thread(thread_id).await {
            let config_snapshot = thread.config_snapshot().await;
            let loaded_thread =
                build_thread_from_snapshot(thread_id, &config_snapshot, thread.rollout_path());
            self.thread_watch_manager.upsert_thread(loaded_thread).await;
        }

        for connection_id in connection_ids {
            Self::log_listener_attach_result(
                self.ensure_conversation_listener(
                    thread_id,
                    connection_id,
                    /*raw_events_enabled*/ false,
                )
                .await,
                thread_id,
                connection_id,
                "thread",
            );
        }
    }

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
            persist_extended_history,
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

        match self
            .thread_manager
            .resume_thread_with_history(
                config,
                thread_history,
                self.auth_manager.clone(),
                persist_extended_history,
                self.request_trace_context(&request_id).await,
            )
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

                self.apply_thread_runtime_state(
                    &mut thread,
                    /*has_live_in_progress_turn*/ false,
                )
                .await;
                let thread_status = thread.status.clone();
                let control_state = thread.control_state.clone();
                set_thread_status_and_interrupt_stale_turns(
                    &mut thread,
                    thread_status,
                    /*has_live_in_progress_turn*/ false,
                    control_state.as_ref(),
                );

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
                    message: format!("error resuming thread: {err}"),
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

            let config_snapshot = existing_thread.config_snapshot().await;
            let mismatch_details = collect_resume_override_mismatches(params, &config_snapshot);
            if !mismatch_details.is_empty() {
                tracing::warn!(
                    "thread/resume overrides ignored for running thread {}: {}",
                    existing_thread_id,
                    mismatch_details.join("; ")
                );
            }
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
                }),
            );
            if listener_command_tx.send(command).is_err() {
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
            let existing_thread_id = match self.parse_thread_id(thread_id) {
                Ok(id) => id,
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return None;
                }
            };

            match find_thread_rollout_path(
                &self.config,
                existing_thread_id,
                ThreadRolloutScope::Any,
            )
            .await
            {
                Ok(path) => path,
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return None;
                }
            }
        };

        match RolloutRecorder::get_rollout_history(&rollout_path).await {
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
                thread.preview = preview_from_rollout_items(items);
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
        hydrate_thread_turns(
            &mut thread,
            ThreadHistorySource::RolloutItems(&history_items),
            /*active_turn*/ None,
        )
        .await?;
        self.attach_thread_name(thread_id, &mut thread).await;
        Ok(thread)
    }

    pub(super) async fn thread_fork(
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
            let existing_thread_id = match self.parse_thread_id(&thread_id) {
                Ok(id) => id,
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            };

            match find_thread_rollout_path(
                &self.config,
                existing_thread_id,
                ThreadRolloutScope::Any,
            )
            .await
            {
                Ok(path) => (path, Some(existing_thread_id)),
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            }
        };

        let directory = praxis_rollout::ThreadDirectory::open(&self.config).await;
        let history_cwd = directory
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
            match read_summary_from_rollout(
                fork_rollout_path.as_path(),
                fallback_model_provider.as_str(),
            )
            .await
            {
                Ok(summary) => summary_to_thread(
                    hydrate_rollout_summary_with_state_db(&self.config, summary).await,
                ),
                Err(err) => {
                    self.send_internal_error(
                        request_id,
                        format!(
                            "failed to load rollout `{}` for thread {thread_id}: {err}",
                            fork_rollout_path.display()
                        ),
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
            let history_items = match read_thread_rollout_items(rollout_path.as_path()).await {
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
            thread.preview = preview_from_rollout_items(&history_items);
            if let Err(message) = hydrate_thread_turns(
                &mut thread,
                ThreadHistorySource::RolloutItems(&history_items),
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
            && let Err(message) = hydrate_thread_turns(
                &mut thread,
                ThreadHistorySource::RolloutPath(fork_rollout_path.as_path()),
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

        self.apply_thread_runtime_state(&mut thread, /*has_live_in_progress_turn*/ false)
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

    async fn list_threads_common(
        &self,
        requested_page_size: usize,
        cursor: Option<String>,
        sort_key: CoreThreadSortKey,
        filters: ThreadListFilters,
    ) -> Result<(Vec<praxis_rollout::ThreadSummary>, Option<String>), JSONRPCErrorError> {
        let ThreadListFilters {
            model_providers,
            source_kinds,
            archived,
            cwd,
            search_term,
        } = filters;
        let cursor_obj: Option<RolloutCursor> = match cursor.as_ref() {
            Some(cursor_str) => {
                Some(parse_cursor(cursor_str).ok_or_else(|| JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("invalid cursor: {cursor_str}"),
                    data: None,
                })?)
            }
            None => None,
        };
        let model_provider_filter =
            model_providers.and_then(|providers| (!providers.is_empty()).then_some(providers));
        let directory = praxis_rollout::ThreadDirectory::open(&self.config).await;
        let page = directory
            .list_threads(praxis_rollout::ListThreadsQuery {
                page_size: requested_page_size.min(THREAD_LIST_MAX_LIMIT as usize),
                cursor: cursor_obj,
                sort_key,
                model_providers: model_provider_filter,
                source_kinds: source_kinds
                    .map(|kinds| kinds.into_iter().map(map_thread_source_kind).collect()),
                archived,
                cwd,
                search_term,
                fallback_provider: self.config.model_provider_id.clone(),
            })
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to list threads: {err}"),
                data: None,
            })?;

        let next_cursor = page
            .next_cursor
            .as_ref()
            .and_then(|cursor| serde_json::to_value(cursor).ok())
            .and_then(|value| value.as_str().map(str::to_owned));
        Ok((page.items, next_cursor))
    }

    pub(super) async fn wait_for_thread_shutdown(
        thread: &Arc<PraxisThread>,
    ) -> ThreadShutdownResult {
        match tokio::time::timeout(Duration::from_secs(10), thread.shutdown_and_wait()).await {
            Ok(Ok(())) => ThreadShutdownResult::Complete,
            Ok(Err(_)) => ThreadShutdownResult::SubmitFailed,
            Err(_) => ThreadShutdownResult::TimedOut,
        }
    }

    pub(super) async fn finalize_thread_teardown(&mut self, thread_id: ThreadId) {
        self.pending_thread_unloads.lock().await.remove(&thread_id);
        self.outgoing
            .cancel_requests_for_thread(thread_id, /*error*/ None)
            .await;
        self.thread_state_manager
            .remove_thread_state(thread_id)
            .await;
        self.thread_watch_manager
            .remove_thread(&thread_id.to_string())
            .await;
    }

    pub(super) async fn thread_unsubscribe(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadUnsubscribeParams,
    ) {
        let thread_id = match self.parse_thread_id(&params.thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let Ok(thread) = self.thread_manager.get_thread(thread_id).await else {
            // Reconcile stale app-gateway bookkeeping when the thread has already been
            // removed from the core manager. This keeps loaded-status/subscription state
            // consistent with the source of truth before reporting NotLoaded.
            self.finalize_thread_teardown(thread_id).await;
            self.outgoing
                .send_response(
                    request_id,
                    ThreadUnsubscribeResponse {
                        status: ThreadUnsubscribeStatus::NotLoaded,
                    },
                )
                .await;
            return;
        };

        let was_subscribed = self
            .thread_state_manager
            .unsubscribe_connection_from_thread(thread_id, request_id.connection_id)
            .await;
        if !was_subscribed {
            self.outgoing
                .send_response(
                    request_id,
                    ThreadUnsubscribeResponse {
                        status: ThreadUnsubscribeStatus::NotSubscribed,
                    },
                )
                .await;
            return;
        }

        if !self.thread_state_manager.has_subscribers(thread_id).await {
            // This connection was the last subscriber. Only now do we unload the thread.
            info!("thread {thread_id} has no subscribers; shutting down");
            self.pending_thread_unloads.lock().await.insert(thread_id);
            // Any pending app-gateway -> client requests for this thread can no longer be
            // answered; cancel their callbacks before shutdown/unload.
            self.outgoing
                .cancel_requests_for_thread(thread_id, /*error*/ None)
                .await;
            self.thread_state_manager
                .remove_thread_state(thread_id)
                .await;

            let outgoing = self.outgoing.clone();
            let pending_thread_unloads = self.pending_thread_unloads.clone();
            let thread_manager = self.thread_manager.clone();
            let thread_watch_manager = self.thread_watch_manager.clone();
            let config = Arc::clone(&self.config);
            tokio::spawn(async move {
                match Self::wait_for_thread_shutdown(&thread).await {
                    ThreadShutdownResult::Complete => {
                        if thread_manager.remove_thread(&thread_id).await.is_none() {
                            info!(
                                "thread {thread_id} was already removed before unsubscribe finalized"
                            );
                            thread_watch_manager
                                .remove_thread(&thread_id.to_string())
                                .await;
                            pending_thread_unloads.lock().await.remove(&thread_id);
                            return;
                        }
                        thread_watch_manager
                            .remove_thread(&thread_id.to_string())
                            .await;
                        let notification = ThreadClosedNotification {
                            thread_id: thread_id.to_string(),
                        };
                        outgoing
                            .send_server_notification(ServerNotification::ThreadClosed(
                                notification,
                            ))
                            .await;
                        pending_thread_unloads.lock().await.remove(&thread_id);
                    }
                    ThreadShutdownResult::SubmitFailed => {
                        pending_thread_unloads.lock().await.remove(&thread_id);
                        warn!("failed to submit Shutdown to thread {thread_id}");
                    }
                    ThreadShutdownResult::TimedOut => {
                        pending_thread_unloads.lock().await.remove(&thread_id);
                        warn!("thread {thread_id} shutdown timed out; leaving thread loaded");
                    }
                }
            });
        }

        self.outgoing
            .send_response(
                request_id,
                ThreadUnsubscribeResponse {
                    status: ThreadUnsubscribeStatus::Unsubscribed,
                },
            )
            .await;
    }
}

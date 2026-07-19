use super::*;

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn thread_start(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadStartParams,
        request_context: RequestContext,
    ) {
        self.thread_start_with_parent(request_id, params, request_context, None)
            .await;
    }

    pub(in crate::praxis_message_processor) async fn thread_child_start(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadChildStartParams,
        request_context: RequestContext,
    ) {
        let ThreadChildStartParams {
            parent_thread_id,
            thread,
            agent_role,
            agent_title,
        } = params;
        let Ok(parent_thread_id) = ThreadId::from_string(&parent_thread_id) else {
            self.send_invalid_request_error(request_id, "invalid parent thread id".to_owned())
                .await;
            return;
        };
        self.thread_start_with_parent(
            request_id,
            thread,
            request_context,
            Some((parent_thread_id, agent_role, agent_title)),
        )
        .await;
    }

    async fn thread_start_with_parent(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadStartParams,
        request_context: RequestContext,
        child: Option<(ThreadId, Option<String>, Option<String>)>,
    ) {
        let ThreadStartParams {
            thread_id,
            model,
            model_provider,
            reasoning_effort,
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
        let requested_thread_id = match thread_id {
            Some(thread_id) => match ThreadId::from_string(&thread_id) {
                Ok(thread_id) => Some(thread_id),
                Err(_) => {
                    self.send_invalid_request_error(
                        request_id,
                        "thread_id must be a valid UUID".to_owned(),
                    )
                    .await;
                    return;
                }
            },
            None => None,
        };
        let mut typesafe_overrides = self.build_thread_config_overrides(
            model,
            model_provider,
            reasoning_effort,
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
            workspace_change_store: self.workspace_change_store.clone(),
            fallback_model_provider: self.config.model_provider_id.clone(),
            praxis_home: self.config.praxis_home.clone(),
            state_db: get_state_db(self.config.as_ref()).await,
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
                child,
                requested_thread_id,
            )
            .await;
        };
        self.background_tasks
            .spawn(thread_start_task.instrument(request_context.span()));
    }

    #[allow(clippy::too_many_arguments)]
    async fn thread_start_task(
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
        child: Option<(ThreadId, Option<String>, Option<String>)>,
        requested_thread_id: Option<ThreadId>,
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

        let core_dynamic_tools = match build_core_dynamic_tools(dynamic_tools) {
            Ok(tools) => tools,
            Err(message) => {
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
        };
        let core_dynamic_tool_count = core_dynamic_tools.len();

        let spawn_result = async {
            match child {
                Some((parent_thread_id, agent_role, agent_title)) => {
                    listener_task_context
                        .thread_manager
                        .start_child_thread_with_tools_and_service_name(
                            parent_thread_id,
                            config,
                            core_dynamic_tools,
                            persist_extended_history,
                            service_name,
                            request_trace,
                            agent_role,
                            agent_title,
                            requested_thread_id,
                        )
                        .await
                }
                None => {
                    listener_task_context
                        .thread_manager
                        .start_thread_with_tools_service_name_and_id(
                            config,
                            core_dynamic_tools,
                            persist_extended_history,
                            service_name,
                            request_trace,
                            requested_thread_id,
                        )
                        .await
                }
            }
        }
        .instrument(tracing::info_span!(
            "app_gateway.thread_start.create_thread",
            otel.name = "app_gateway.thread_start.create_thread",
            thread_start.dynamic_tool_count = core_dynamic_tool_count,
            thread_start.persist_extended_history = persist_extended_history,
        ))
        .await;

        match spawn_result {
            Ok(new_conv) => {
                let ThreadSpawnResult {
                    thread_id,
                    thread: core_thread,
                    session_configured,
                    initial_config_snapshot: config_snapshot,
                    ..
                } = new_conv;
                let mut thread = build_thread_from_snapshot(
                    thread_id,
                    &config_snapshot,
                    session_configured.rollout_path.clone(),
                );
                tracing::info!(%thread_id, "thread start response snapshot built");
                thread.status = praxis_app_gateway_protocol::ThreadStatus::Idle;

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
                    history_log_id: session_configured.history_log_id,
                    history_entry_count: u64::try_from(session_configured.history_entry_count)
                        .unwrap_or(u64::MAX),
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
                tracing::info!(%thread_id, "thread start analytics recorded");

                let continuation_context = listener_task_context.clone();
                let continuation_thread = thread.clone();
                let connection_id = request_id.connection_id;
                tokio::spawn(async move {
                    Self::log_listener_attach_result(
                        Self::ensure_conversation_listener_for_thread_task(
                            continuation_context.clone(),
                            thread_id,
                            core_thread,
                            connection_id,
                            experimental_raw_events,
                        )
                        .instrument(tracing::info_span!(
                            "app_gateway.thread_start.attach_listener",
                            otel.name = "app_gateway.thread_start.attach_listener",
                            thread_start.experimental_raw_events = experimental_raw_events,
                        ))
                        .await,
                        thread_id,
                        connection_id,
                        "thread",
                    );

                    continuation_context
                        .thread_watch_manager
                        .upsert_thread_silently(continuation_thread.clone())
                        .instrument(tracing::info_span!(
                            "app_gateway.thread_start.upsert_thread",
                            otel.name = "app_gateway.thread_start.upsert_thread",
                        ))
                        .await;

                    let notif = ThreadStartedNotification {
                        thread: continuation_thread,
                    };
                    continuation_context
                        .outgoing
                        .send_server_notification(ServerNotification::ThreadStarted(notif))
                        .instrument(tracing::info_span!(
                            "app_gateway.thread_start.notify_started",
                            otel.name = "app_gateway.thread_start.notify_started",
                        ))
                        .await;
                });

                tracing::info!(%thread_id, request_id = ?request_id.request_id, "thread start enqueueing response");
                listener_task_context
                    .outgoing
                    .send_response(request_id, response)
                    .instrument(tracing::info_span!(
                        "app_gateway.thread_start.send_response",
                        otel.name = "app_gateway.thread_start.send_response",
                    ))
                    .await;
                tracing::info!(%thread_id, "thread start response enqueued");
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("error creating thread: {err:#}"),
                    data: None,
                };
                listener_task_context
                    .outgoing
                    .send_error(request_id, error)
                    .await;
            }
        }
    }
}

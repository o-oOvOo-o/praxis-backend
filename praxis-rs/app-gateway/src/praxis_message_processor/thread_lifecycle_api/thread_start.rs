use super::*;

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn thread_start(
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

                project_thread_runtime_state_from_watch(
                    &listener_task_context.thread_watch_manager,
                    &mut thread,
                    /*has_live_in_progress_turn*/ false,
                )
                .instrument(tracing::info_span!(
                    "app_gateway.thread_start.resolve_status",
                    otel.name = "app_gateway.thread_start.resolve_status",
                ))
                .await;

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

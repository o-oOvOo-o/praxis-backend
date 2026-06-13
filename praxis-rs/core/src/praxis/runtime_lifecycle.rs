use super::*;

impl Praxis {
    /// Spawn a new [`Praxis`] and initialize the session.
    pub(crate) async fn spawn(args: PraxisSpawnArgs) -> PraxisResult<PraxisSpawnOk> {
        let parent_trace = match args.parent_trace {
            Some(trace) => {
                if praxis_otel::context_from_w3c_trace_context(&trace).is_some() {
                    Some(trace)
                } else {
                    warn!("ignoring invalid thread spawn trace carrier");
                    None
                }
            }
            None => None,
        };
        let thread_spawn_span = info_span!("thread_spawn", otel.name = "thread_spawn");
        if let Some(trace) = parent_trace.as_ref() {
            let _ = set_parent_from_w3c_trace_context(&thread_spawn_span, trace);
        }
        Self::spawn_internal(PraxisSpawnArgs {
            parent_trace,
            ..args
        })
        .instrument(thread_spawn_span)
        .await
    }

    async fn spawn_internal(args: PraxisSpawnArgs) -> PraxisResult<PraxisSpawnOk> {
        let PraxisSpawnArgs {
            mut config,
            auth_manager,
            models_manager,
            environment_manager,
            skills_manager,
            plugins_manager,
            mcp_manager,
            skills_watcher,
            conversation_history,
            session_source,
            agent_control,
            agent_os,
            dynamic_tools,
            persist_extended_history,
            metrics_service_name,
            inherited_shell_snapshot,
            user_shell_override,
            inherited_exec_policy,
            parent_trace: _,
        } = args;
        let (tx_sub, rx_sub) = async_channel::bounded(SUBMISSION_CHANNEL_CAPACITY);
        let (tx_event, rx_event) = async_channel::unbounded();

        let plugin_outcome = plugins_manager.plugins_for_config(&config);
        let effective_skill_roots = plugin_outcome.effective_skill_roots();
        let llm_runtime_catalog =
            LlmRuntimeCatalog::from_plugin_manifests(plugin_outcome.effective_llm_manifests());
        merge_plugin_model_catalog(&mut config, &llm_runtime_catalog);
        let skills_input = skills_load_input_from_config(&config, effective_skill_roots);
        let loaded_skills = skills_manager.skills_for_config(&skills_input);

        for err in &loaded_skills.errors {
            error!(
                "failed to load skill {}: {}",
                err.path.display(),
                err.message
            );
        }

        if let SessionSource::SubAgent(SubAgentSource::ThreadSpawn { depth, .. }) = session_source
            && depth >= config.agent_max_depth
        {
            let _ = config.features.disable(Feature::SpawnCsv);
            let _ = config.features.disable(Feature::Collab);
        }

        if config.features.enabled(Feature::JsRepl)
            && let Err(err) = resolve_compatible_node(config.js_repl_node_path.as_deref()).await
        {
            let _ = config.features.disable(Feature::JsRepl);
            let _ = config.features.disable(Feature::JsReplToolsOnly);
            let message = if config.features.enabled(Feature::JsRepl) {
                format!(
                    "`js_repl` remains enabled because enterprise requirements pin it on, but the configured Node runtime is unavailable or incompatible. {err}"
                )
            } else {
                format!(
                    "Disabled `js_repl` for this session because the configured Node runtime is unavailable or incompatible. {err}"
                )
            };
            warn!("{message}");
            config.startup_warnings.push(message);
        }
        if config.features.enabled(Feature::CodeMode)
            && let Err(err) = resolve_compatible_node(config.js_repl_node_path.as_deref()).await
        {
            let message = format!(
                "Disabled `exec` for this session because the configured Node runtime is unavailable or incompatible. {err}"
            );
            warn!("{message}");
            let _ = config.features.disable(Feature::CodeMode);
            config.startup_warnings.push(message);
        }

        let user_instructions = get_user_instructions(&config).await;

        let exec_policy = if crate::guardian::is_guardian_reviewer_source(&session_source) {
            // Guardian review should rely on the built-in shell safety checks,
            // not on caller-provided exec-policy rules that could shape the
            // reviewer or silently auto-approve commands.
            Arc::new(ExecPolicyManager::default())
        } else if let Some(exec_policy) = &inherited_exec_policy {
            Arc::clone(exec_policy)
        } else {
            Arc::new(
                ExecPolicyManager::load(&config.config_layer_stack)
                    .await
                    .map_err(|err| PraxisErr::Fatal(format!("failed to load rules: {err}")))?,
            )
        };

        let config = Arc::new(config);
        let refresh_strategy = match session_source {
            SessionSource::SubAgent(_) => crate::models_manager::manager::RefreshStrategy::Offline,
            _ => crate::models_manager::manager::RefreshStrategy::OnlineIfUncached,
        };
        if config.model.is_none()
            || !matches!(
                refresh_strategy,
                crate::models_manager::manager::RefreshStrategy::Offline
            )
        {
            let _ = models_manager
                .list_models_for_config(&config, refresh_strategy)
                .await;
        }
        let model = models_manager
            .get_default_model_for_config(&config.model, refresh_strategy, &config)
            .await;

        // Resolve base instructions for the session. Priority order:
        // 1. config.base_instructions override
        // 2. conversation history => session_meta.base_instructions
        // 3. prompt profile for current provider/model
        // 4. base_instructions for current model
        let model_info = models_manager.get_model_info(model.as_str(), &config).await;
        let base_instructions = config
            .base_instructions
            .clone()
            .or_else(|| conversation_history.get_base_instructions().map(|s| s.text))
            .unwrap_or_else(|| {
                crate::prompt_profiles::resolve_model_instructions(
                    &model_info,
                    &config.model_provider_id,
                    &config.model_provider,
                    config.personality,
                    session_source
                        .restriction_product()
                        .and_then(crate::llm::ids::ProductProfileId::from_product),
                    &llm_runtime_catalog,
                )
            });

        // Respect thread-start tools. When missing (resumed/forked threads), read from the db
        // first, then fall back to rollout-file tools.
        let persisted_tools = if dynamic_tools.is_empty() {
            let thread_id = match &conversation_history {
                InitialHistory::Resumed(resumed) => Some(resumed.conversation_id),
                InitialHistory::Forked(_) => conversation_history.forked_from_id(),
                InitialHistory::New => None,
            };
            match thread_id {
                Some(thread_id) => {
                    let state_db_ctx = state_db::get_state_db(&config).await;
                    state_db::get_dynamic_tools(state_db_ctx.as_deref(), thread_id, "praxis_spawn")
                        .await
                }
                None => None,
            }
        } else {
            None
        };
        let dynamic_tools = if dynamic_tools.is_empty() {
            persisted_tools
                .or_else(|| conversation_history.get_dynamic_tools())
                .unwrap_or_default()
        } else {
            dynamic_tools
        };

        // TODO (aibrahim): Consolidate config.model and config.model_reasoning_effort into config.collaboration_mode
        // to avoid extracting these fields separately and constructing CollaborationMode here.
        let collaboration_mode = CollaborationMode {
            mode: ModeKind::Default,
            settings: Settings {
                model: model.clone(),
                reasoning_effort: config.model_reasoning_effort,
                developer_instructions: None,
            },
        };
        let session_configuration = SessionConfiguration {
            provider: config.model_provider.clone(),
            collaboration_mode,
            model_reasoning_summary: config.model_reasoning_summary,
            service_tier: config.service_tier,
            developer_instructions: config.developer_instructions.clone(),
            user_instructions,
            personality: config.personality,
            base_instructions,
            compact_prompt: config.compact_prompt.clone(),
            approval_policy: config.permissions.approval_policy.clone(),
            approvals_reviewer: config.approvals_reviewer,
            sandbox_policy: config.permissions.sandbox_policy.clone(),
            file_system_sandbox_policy: config.permissions.file_system_sandbox_policy.clone(),
            network_sandbox_policy: config.permissions.network_sandbox_policy,
            windows_sandbox_level: WindowsSandboxLevel::from_config(&config),
            cwd: config.cwd.clone(),
            praxis_home: config.praxis_home.clone(),
            thread_name: None,
            original_config_do_not_use: Arc::clone(&config),
            metrics_service_name,
            app_gateway_client_name: None,
            session_source,
            dynamic_tools,
            persist_extended_history,
            inherited_shell_snapshot,
            user_shell_override,
        };

        // Generate a unique ID for the lifetime of this Praxis session.
        let session_source_clone = session_configuration.session_source.clone();
        let (agent_status_tx, agent_status_rx) = watch::channel(AgentStatus::PendingInit);

        let session = Session::new(
            session_configuration,
            llm_runtime_catalog,
            config.clone(),
            auth_manager.clone(),
            models_manager.clone(),
            exec_policy,
            tx_event.clone(),
            agent_status_tx.clone(),
            conversation_history,
            session_source_clone,
            environment_manager,
            skills_manager,
            plugins_manager,
            mcp_manager.clone(),
            skills_watcher,
            agent_control,
            agent_os,
        )
        .await
        .map_err(|e| {
            error!("Failed to create session: {e:#}");
            map_session_init_error(&e, &config.praxis_home)
        })?;
        let thread_id = session.conversation_id;

        // This task will run until Op::Shutdown is received.
        let session_for_loop = Arc::clone(&session);
        let session_loop_handle = tokio::spawn(async move {
            submission_loop(session_for_loop, config, rx_sub)
                .instrument(info_span!("session_loop", thread_id = %thread_id))
                .await;
        });
        let praxis = Praxis {
            tx_sub,
            rx_event,
            agent_status: agent_status_rx,
            session,
            session_loop_termination: session_loop_termination_from_handle(session_loop_handle),
        };

        Ok(PraxisSpawnOk { praxis, thread_id })
    }

    /// Submit the `op` wrapped in a `Submission` with a unique ID.
    pub async fn submit(&self, op: Op) -> PraxisResult<String> {
        self.submit_with_trace(op, /*trace*/ None).await
    }

    pub async fn submit_with_trace(
        &self,
        op: Op,
        trace: Option<W3cTraceContext>,
    ) -> PraxisResult<String> {
        let id = Uuid::now_v7().to_string();
        let sub = Submission {
            id: id.clone(),
            op,
            trace,
        };
        self.submit_with_id(sub).await?;
        Ok(id)
    }

    /// Use sparingly: prefer `submit()` so Praxis is responsible for generating
    /// unique IDs for each submission.
    pub async fn submit_with_id(&self, mut sub: Submission) -> PraxisResult<()> {
        if sub.trace.is_none() {
            sub.trace = current_span_w3c_trace_context();
        }
        self.tx_sub
            .send(sub)
            .await
            .map_err(|_| PraxisErr::InternalAgentDied)?;
        Ok(())
    }

    pub async fn shutdown_and_wait(&self) -> PraxisResult<()> {
        let session_loop_termination = self.session_loop_termination.clone();
        match self.submit(Op::Shutdown).await {
            Ok(_) => {}
            Err(PraxisErr::InternalAgentDied) => {}
            Err(err) => return Err(err),
        }
        session_loop_termination.await;
        Ok(())
    }

    pub async fn next_event(&self) -> PraxisResult<Event> {
        let event = self
            .rx_event
            .recv()
            .await
            .map_err(|_| PraxisErr::InternalAgentDied)?;
        Ok(event)
    }

    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
    ) -> Result<String, SteerInputError> {
        self.session.steer_input(input, expected_turn_id).await
    }

    pub(crate) async fn set_app_gateway_client_name(
        &self,
        app_gateway_client_name: Option<String>,
    ) -> ConstraintResult<()> {
        self.session
            .update_settings(SessionSettingsUpdate {
                app_gateway_client_name,
                ..Default::default()
            })
            .await
    }

    pub(crate) async fn agent_status(&self) -> AgentStatus {
        self.agent_status.borrow().clone()
    }

    pub(crate) async fn thread_config_snapshot(&self) -> ThreadConfigSnapshot {
        let state = self.session.state.lock().await;
        state.session_configuration.thread_config_snapshot()
    }

    pub(crate) fn state_db(&self) -> Option<state_db::StateDbHandle> {
        self.session.state_db()
    }

    pub(crate) fn enabled(&self, feature: Feature) -> bool {
        self.session.enabled(feature)
    }
}

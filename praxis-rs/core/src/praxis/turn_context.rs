use super::*;

impl Session {
    /// Don't expand the number of mutated arguments on config. We are in the process of getting rid of it.
    pub(crate) fn build_per_turn_config(session_configuration: &SessionConfiguration) -> Config {
        // todo(aibrahim): store this state somewhere else so we don't need to mut config
        let config = session_configuration.original_config_do_not_use.clone();
        let mut per_turn_config = (*config).clone();
        per_turn_config.cwd = session_configuration.cwd.clone();
        per_turn_config.model_reasoning_effort =
            session_configuration.collaboration_mode.reasoning_effort();
        per_turn_config.model_reasoning_summary = session_configuration.model_reasoning_summary;
        per_turn_config.service_tier = session_configuration.service_tier;
        per_turn_config.personality = session_configuration.personality;
        per_turn_config.approvals_reviewer = session_configuration.approvals_reviewer;
        let resolved_web_search_mode = resolve_web_search_mode_for_turn(
            &per_turn_config.web_search_mode,
            session_configuration.sandbox_policy.get(),
        );
        if let Err(err) = per_turn_config
            .web_search_mode
            .set(resolved_web_search_mode)
        {
            let fallback_value = per_turn_config.web_search_mode.value();
            tracing::warn!(
                error = %err,
                ?resolved_web_search_mode,
                ?fallback_value,
                "resolved web_search_mode is disallowed by requirements; keeping constrained value"
            );
        }
        per_turn_config.features = config.features.clone();
        per_turn_config
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn make_turn_context(
        conversation_id: ThreadId,
        auth_manager: Option<Arc<AuthManager>>,
        session_telemetry: &SessionTelemetry,
        provider: ModelProviderInfo,
        session_configuration: &SessionConfiguration,
        user_shell: &shell::Shell,
        shell_zsh_path: Option<&PathBuf>,
        main_execve_wrapper_exe: Option<&PathBuf>,
        per_turn_config: Config,
        model_info: ModelInfo,
        models_manager: &ModelsManager,
        llm_runtime_catalog: &LlmRuntimeCatalog,
        network: Option<NetworkProxy>,
        environment: Arc<Environment>,
        sub_id: String,
        skills_outcome: Arc<SkillLoadOutcome>,
    ) -> TurnContext {
        let reasoning_effort = session_configuration.collaboration_mode.reasoning_effort();
        let reasoning_summary = session_configuration
            .model_reasoning_summary
            .unwrap_or(model_info.default_reasoning_summary);
        let session_telemetry = session_telemetry.clone().with_model(
            session_configuration.collaboration_mode.model(),
            model_info.slug.as_str(),
        );
        let session_source = session_configuration.session_source.clone();
        let auth_manager_for_context = auth_manager;
        let provider_for_context = provider;
        let session_telemetry_for_context = session_telemetry;
        let tool_capabilities = tool_capabilities_for_turn_model(
            llm_runtime_catalog,
            &model_info,
            per_turn_config.model_provider_id.as_str(),
            &provider_for_context,
            &session_source,
        );
        let tools_config = ToolsConfig::new(&ToolsConfigParams {
            model_info: &model_info,
            available_models: &models_manager
                .try_list_models_for_config(&per_turn_config)
                .unwrap_or_default(),
            features: &per_turn_config.features,
            web_search_mode: Some(per_turn_config.web_search_mode.value()),
            session_source: session_source.clone(),
            sandbox_policy: session_configuration.sandbox_policy.get(),
            windows_sandbox_level: session_configuration.windows_sandbox_level,
        })
        .with_tool_wire_profile(tool_wire_profile_for_wire_api(
            provider_for_context.wire_api,
        ))
        .with_tool_capabilities(tool_capabilities)
        .with_unified_exec_shell_mode_for_session(
            crate::tools::spec::tool_user_shell_type(user_shell),
            shell_zsh_path,
            main_execve_wrapper_exe,
        )
        .with_web_search_config(per_turn_config.web_search_config.clone())
        .with_allow_login_shell(per_turn_config.permissions.allow_login_shell)
        .with_agent_type_description(crate::agent::role::spawn_tool_spec::build(
            &per_turn_config.agent_roles,
        ));

        let cwd = session_configuration.cwd.clone();

        let per_turn_config = Arc::new(per_turn_config);
        let turn_metadata_state = Arc::new(TurnMetadataState::new(
            conversation_id.to_string(),
            sub_id.clone(),
            cwd.to_path_buf(),
            session_configuration.sandbox_policy.get(),
            session_configuration.windows_sandbox_level,
        ));
        let (current_date, timezone) = local_time_context();
        TurnContext {
            sub_id,
            trace_id: current_span_trace_id(),
            realtime_active: false,
            config: per_turn_config.clone(),
            auth_manager: auth_manager_for_context,
            model_info: model_info.clone(),
            session_telemetry: session_telemetry_for_context,
            provider: provider_for_context,
            reasoning_effort,
            reasoning_summary,
            session_source,
            environment,
            cwd,
            current_date: Some(current_date),
            timezone: Some(timezone),
            app_gateway_client_name: session_configuration.app_gateway_client_name.clone(),
            developer_instructions: session_configuration.developer_instructions.clone(),
            compact_prompt: session_configuration.compact_prompt.clone(),
            user_instructions: session_configuration.user_instructions.clone(),
            collaboration_mode: session_configuration.collaboration_mode.clone(),
            personality: session_configuration.personality,
            approval_policy: session_configuration.approval_policy.clone(),
            sandbox_policy: session_configuration.sandbox_policy.clone(),
            file_system_sandbox_policy: session_configuration.file_system_sandbox_policy.clone(),
            network_sandbox_policy: session_configuration.network_sandbox_policy,
            network,
            windows_sandbox_level: session_configuration.windows_sandbox_level,
            shell_environment_policy: per_turn_config.permissions.shell_environment_policy.clone(),
            tools_config,
            features: per_turn_config.features.clone(),
            ghost_snapshot: per_turn_config.ghost_snapshot.clone(),
            final_output_json_schema: None,
            praxis_self_exe: per_turn_config.praxis_self_exe.clone(),
            praxis_linux_sandbox_exe: per_turn_config.praxis_linux_sandbox_exe.clone(),
            tool_call_gate: Arc::new(ReadinessFlag::new()),
            tool_loop_guard: Arc::new(ToolLoopGuardState::default()),
            truncation_policy: model_info.truncation_policy.into(),
            dynamic_tools: session_configuration.dynamic_tools.clone(),
            turn_metadata_state,
            turn_skills: TurnSkillsContext::new(skills_outcome),
            turn_timing_state: Arc::new(TurnTimingState::default()),
        }
    }

    pub(super) fn maybe_refresh_shell_snapshot_for_cwd(
        &self,
        previous_cwd: &Path,
        next_cwd: &Path,
        praxis_home: &Path,
        session_source: &SessionSource,
    ) {
        if previous_cwd == next_cwd {
            return;
        }

        if !self.features.enabled(Feature::ShellSnapshot) {
            return;
        }

        if matches!(
            session_source,
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. })
        ) {
            return;
        }

        ShellSnapshot::refresh_snapshot(
            praxis_home.to_path_buf(),
            self.conversation_id,
            next_cwd.to_path_buf(),
            self.services.user_shell.as_ref().clone(),
            self.services.shell_snapshot_tx.clone(),
            self.services.session_telemetry.clone(),
        );
    }

    pub(crate) async fn update_settings(
        &self,
        updates: SessionSettingsUpdate,
    ) -> ConstraintResult<()> {
        let (previous_configuration, mut updated) = {
            let state = self.state.lock().await;
            let previous_configuration = state.session_configuration.clone();
            let updated = match previous_configuration.apply(&updates) {
                Ok(updated) => updated,
                Err(err) => {
                    warn!("rejected session settings update: {err}");
                    return Err(err);
                }
            };
            (previous_configuration, updated)
        };

        if Self::prompt_route_update_needed(&previous_configuration, &updated, &updates) {
            self.refresh_model_base_instructions(&mut updated).await;
        }

        let previous_cwd = previous_configuration.cwd.clone();
        let next_cwd = updated.cwd.clone();
        let praxis_home = updated.praxis_home.clone();
        let session_source = updated.session_source.clone();
        {
            let mut state = self.state.lock().await;
            state.session_configuration = updated;
        }

        self.maybe_refresh_shell_snapshot_for_cwd(
            &previous_cwd,
            &next_cwd,
            &praxis_home,
            &session_source,
        );

        Ok(())
    }

    pub(crate) async fn new_turn_with_sub_id(
        &self,
        sub_id: String,
        updates: SessionSettingsUpdate,
    ) -> ConstraintResult<Arc<TurnContext>> {
        let (
            previous_configuration,
            session_configuration,
            sandbox_policy_changed,
            previous_cwd,
            praxis_home,
            session_source,
        ) = {
            let state = self.state.lock().await;
            let previous_configuration = state.session_configuration.clone();
            match previous_configuration.apply(&updates) {
                Ok(next) => {
                    let sandbox_policy_changed =
                        previous_configuration.sandbox_policy != next.sandbox_policy;
                    let previous_cwd = previous_configuration.cwd.clone();
                    let praxis_home = next.praxis_home.clone();
                    let session_source = next.session_source.clone();
                    (
                        previous_configuration,
                        next,
                        sandbox_policy_changed,
                        previous_cwd,
                        praxis_home,
                        session_source,
                    )
                }
                Err(err) => {
                    drop(state);
                    self.raw_event_emitter(sub_id.clone())
                        .error(err.to_string(), Some(CodexErrorInfo::BadRequest))
                        .await;
                    return Err(err);
                }
            }
        };

        let mut session_configuration = session_configuration;
        if Self::prompt_route_update_needed(
            &previous_configuration,
            &session_configuration,
            &updates,
        ) {
            self.refresh_model_base_instructions(&mut session_configuration)
                .await;
        }
        {
            let mut state = self.state.lock().await;
            state.session_configuration = session_configuration.clone();
        }

        self.maybe_refresh_shell_snapshot_for_cwd(
            &previous_cwd,
            &session_configuration.cwd,
            &praxis_home,
            &session_source,
        );

        Ok(self
            .new_turn_from_configuration(
                sub_id,
                session_configuration,
                updates.final_output_json_schema,
                sandbox_policy_changed,
            )
            .await)
    }

    pub(super) fn prompt_route_update_needed(
        previous: &SessionConfiguration,
        next: &SessionConfiguration,
        updates: &SessionSettingsUpdate,
    ) -> bool {
        updates.model_provider.is_some()
            || updates.personality.is_some()
            || updates.collaboration_mode.as_ref().is_some_and(|_| {
                previous.collaboration_mode.model() != next.collaboration_mode.model()
            })
    }

    pub(super) async fn refresh_model_base_instructions(
        &self,
        session_configuration: &mut SessionConfiguration,
    ) {
        let per_turn_config = Self::build_per_turn_config(session_configuration);
        if let Some(base_instructions) = per_turn_config.base_instructions.clone() {
            session_configuration.base_instructions = base_instructions;
            return;
        }

        let model = session_configuration.collaboration_mode.model().to_string();
        let model_info = self
            .services
            .models_manager
            .get_model_info(model.as_str(), &per_turn_config)
            .await;
        let product_profile = session_configuration
            .session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product);
        session_configuration.base_instructions =
            crate::prompt_profiles::resolve_model_instructions(
                &model_info,
                per_turn_config.model_provider_id.as_str(),
                &per_turn_config.model_provider,
                session_configuration.personality,
                product_profile,
                &self.llm_runtime_catalog,
            );
    }

    pub(super) async fn new_turn_from_configuration(
        &self,
        sub_id: String,
        session_configuration: SessionConfiguration,
        final_output_json_schema: Option<Option<Value>>,
        sandbox_policy_changed: bool,
    ) -> Arc<TurnContext> {
        let per_turn_config = Self::build_per_turn_config(&session_configuration);
        self.services
            .mcp_connection_manager
            .read()
            .await
            .set_approval_policy(&session_configuration.approval_policy);

        if sandbox_policy_changed {
            let sandbox_state = SandboxState {
                sandbox_policy: per_turn_config.permissions.sandbox_policy.get().clone(),
                praxis_linux_sandbox_exe: per_turn_config.praxis_linux_sandbox_exe.clone(),
                sandbox_cwd: per_turn_config.cwd.to_path_buf(),
                use_legacy_landlock: per_turn_config.features.use_legacy_landlock(),
            };
            if let Err(e) = self
                .services
                .mcp_connection_manager
                .read()
                .await
                .notify_sandbox_state_change(&sandbox_state)
                .await
            {
                warn!("Failed to notify sandbox state change to MCP servers: {e:#}");
            }
        }

        let model_info = self
            .services
            .models_manager
            .get_model_info(
                session_configuration.collaboration_mode.model(),
                &per_turn_config,
            )
            .await;
        let plugin_outcome = self
            .services
            .plugins_manager
            .plugins_for_config(&per_turn_config);
        let effective_skill_roots = plugin_outcome.effective_skill_roots();
        let skills_input = skills_load_input_from_config(&per_turn_config, effective_skill_roots);
        let skills_outcome = Arc::new(
            self.services
                .skills_manager
                .skills_for_config(&skills_input),
        );
        let mut turn_context: TurnContext = Self::make_turn_context(
            self.conversation_id,
            Some(Arc::clone(&self.services.auth_manager)),
            &self.services.session_telemetry,
            session_configuration.provider.clone(),
            &session_configuration,
            self.services.user_shell.as_ref(),
            self.services.shell_zsh_path.as_ref(),
            self.services.main_execve_wrapper_exe.as_ref(),
            per_turn_config,
            model_info,
            &self.services.models_manager,
            &self.llm_runtime_catalog,
            self.services
                .network_proxy
                .as_ref()
                .map(StartedNetworkProxy::proxy),
            Arc::clone(&self.services.environment),
            sub_id,
            skills_outcome,
        );
        turn_context.realtime_active = self.conversation.running_state().await.is_some();

        if let Some(final_schema) = final_output_json_schema {
            turn_context.final_output_json_schema = final_schema;
        }
        let turn_context = Arc::new(turn_context);
        turn_context.turn_metadata_state.spawn_git_enrichment_task();
        turn_context
    }

    pub(crate) async fn maybe_emit_unknown_model_warning_for_turn(&self, tc: &TurnContext) {
        if tc.model_info.used_fallback_model_metadata {
            self.turn_event_emitter(tc)
                .warning(format!(
                    "Model metadata for `{}` not found. Defaulting to fallback metadata; this can degrade performance and cause issues.",
                    tc.model_info.slug
                ))
                .await;
        }
    }

    pub(crate) async fn new_default_turn(&self) -> Arc<TurnContext> {
        self.new_default_turn_with_sub_id(self.next_internal_sub_id())
            .await
    }

    pub(crate) async fn set_session_startup_prewarm(
        &self,
        startup_prewarm: SessionStartupPrewarmHandle,
    ) {
        let mut state = self.state.lock().await;
        state.set_session_startup_prewarm(startup_prewarm);
    }

    pub(crate) async fn take_session_startup_prewarm(&self) -> Option<SessionStartupPrewarmHandle> {
        let mut state = self.state.lock().await;
        state.take_session_startup_prewarm()
    }

    pub(crate) async fn get_config(&self) -> std::sync::Arc<Config> {
        let state = self.state.lock().await;
        state
            .session_configuration
            .original_config_do_not_use
            .clone()
    }

    pub(crate) async fn provider(&self) -> ModelProviderInfo {
        let state = self.state.lock().await;
        state.session_configuration.provider.clone()
    }

    pub(crate) async fn reload_user_config_layer(&self) {
        let config_toml_path = {
            let state = self.state.lock().await;
            state
                .session_configuration
                .praxis_home
                .join(CONFIG_TOML_FILE)
        };

        let user_config = match std::fs::read_to_string(&config_toml_path) {
            Ok(contents) => match toml::from_str::<toml::Value>(&contents) {
                Ok(config) => config,
                Err(err) => {
                    warn!("failed to parse user config while reloading layer: {err}");
                    return;
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                toml::Value::Table(Default::default())
            }
            Err(err) => {
                warn!("failed to read user config while reloading layer: {err}");
                return;
            }
        };

        let config_toml_path = match AbsolutePathBuf::try_from(config_toml_path) {
            Ok(path) => path,
            Err(err) => {
                warn!("failed to resolve user config path while reloading layer: {err}");
                return;
            }
        };

        let mut state = self.state.lock().await;
        let mut config = (*state.session_configuration.original_config_do_not_use).clone();
        config.config_layer_stack = config
            .config_layer_stack
            .with_user_config(&config_toml_path, user_config);
        state.session_configuration.original_config_do_not_use = Arc::new(config);
        self.services.skills_manager.clear_cache();
        self.services.plugins_manager.clear_cache();
    }

    pub(crate) async fn new_default_turn_with_sub_id(&self, sub_id: String) -> Arc<TurnContext> {
        let session_configuration = {
            let state = self.state.lock().await;
            state.session_configuration.clone()
        };
        self.new_turn_from_configuration(
            sub_id,
            session_configuration,
            /*final_output_json_schema*/ None,
            /*sandbox_policy_changed*/ false,
        )
        .await
    }

    pub(super) async fn build_settings_update_items(
        &self,
        reference_context_item: Option<&TurnContextItem>,
        current_context: &TurnContext,
    ) -> Vec<ResponseItem> {
        // TODO: Make context updates a pure diff of persisted previous/current TurnContextItem
        // state so replay/backtracking is deterministic. Runtime inputs that affect model-visible
        // context (shell, exec policy, feature gates, previous-turn bridge) should be persisted
        // state or explicit non-state replay events.
        let previous_turn_settings = {
            let state = self.state.lock().await;
            state.previous_turn_settings()
        };
        let shell = self.user_shell();
        let exec_policy = self.services.exec_policy.current();
        crate::context_manager::updates::build_settings_update_items(
            reference_context_item,
            previous_turn_settings.as_ref(),
            current_context,
            shell.as_ref(),
            exec_policy.as_ref(),
            self.features.enabled(Feature::Personality),
        )
    }
}

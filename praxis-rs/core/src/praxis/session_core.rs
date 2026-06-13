use super::*;

impl Session {
    pub(crate) async fn praxis_home(&self) -> PathBuf {
        let state = self.state.lock().await;
        state.session_configuration.praxis_home().clone()
    }

    pub(crate) fn subscribe_out_of_band_elicitation_pause_state(&self) -> watch::Receiver<bool> {
        self.out_of_band_elicitation_paused.subscribe()
    }

    pub(crate) fn set_out_of_band_elicitation_pause_state(&self, paused: bool) {
        self.out_of_band_elicitation_paused.send_replace(paused);
    }

    pub(crate) fn get_tx_event(&self) -> Sender<Event> {
        self.tx_event.clone()
    }

    pub(crate) fn state_db(&self) -> Option<state_db::StateDbHandle> {
        self.services.state_db.clone()
    }

    pub(crate) async fn original_config(&self) -> Arc<Config> {
        let state = self.state.lock().await;
        Arc::clone(&state.session_configuration.original_config_do_not_use)
    }

    /// Ensure rollout file writes are durably flushed.
    pub(crate) async fn flush_rollout(&self) {
        let recorder = {
            let guard = self.services.rollout.lock().await;
            guard.clone()
        };
        if let Some(rec) = recorder
            && let Err(e) = rec.flush().await
        {
            warn!("failed to flush rollout recorder: {e}");
        }
    }

    pub(crate) async fn ensure_rollout_materialized(&self) {
        let recorder = {
            let guard = self.services.rollout.lock().await;
            guard.clone()
        };
        if let Some(rec) = recorder
            && let Err(e) = rec.persist().await
        {
            warn!("failed to materialize rollout recorder: {e}");
        }
    }

    pub(crate) fn next_internal_sub_id(&self) -> String {
        let id = self
            .next_internal_sub_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("auto-{id}")
    }

    /// Returns the current thread name, if set.
    pub(crate) async fn thread_name(&self) -> Option<String> {
        let state = self.state.lock().await;
        state.session_configuration.thread_name.clone()
    }

    /// Returns whether thread metadata can be persisted for this session.
    pub(crate) async fn thread_name_persistence_enabled(&self) -> bool {
        let rollout = self.services.rollout.lock().await;
        rollout.is_some() && self.services.state_db.is_some()
    }

    pub(crate) fn llm_runtime_catalog(&self) -> &LlmRuntimeCatalog {
        &self.llm_runtime_catalog
    }

    /// Returns the current model runtime context for internal metadata requests.
    pub(crate) async fn auto_title_model_context(&self) -> AutoTitleModelContext {
        let session_configuration = {
            let state = self.state.lock().await;
            state.session_configuration.clone()
        };
        let per_turn_config = Self::build_per_turn_config(&session_configuration);
        let product_profile = session_configuration
            .session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product);
        let current_model_slug = session_configuration.collaboration_mode.model().to_string();
        let current_model_info = self
            .services
            .models_manager
            .get_model_info(current_model_slug.as_str(), &per_turn_config)
            .await;
        let mut selection = select_auto_title_model(
            &current_model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
        );
        if let Some(auto_title_policy) = self.llm_runtime_catalog.auto_title_task_policy_for_model(
            &current_model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
            product_profile,
        ) {
            if let Some(model_slug) = auto_title_policy.model_slug {
                selection.model_slug = model_slug;
            }
            if let Some(reasoning_effort) = auto_title_policy.reasoning_effort {
                selection.reasoning_effort = Some(reasoning_effort);
            }
            if let Some(suppress_model_default_reasoning) =
                auto_title_policy.suppress_model_default_reasoning
            {
                selection.suppress_model_default_reasoning = suppress_model_default_reasoning;
            }
        }
        let instructions = self.llm_runtime_catalog.resolve_prompt_for_model(
            &current_model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
            product_profile,
            LlmPromptPurpose::AutoTitle,
        );
        let mut title_model_info = if selection.model_slug == current_model_info.slug {
            current_model_info
        } else {
            self.services
                .models_manager
                .get_model_info(selection.model_slug.as_str(), &per_turn_config)
                .await
        };
        if selection.suppress_model_default_reasoning {
            title_model_info.default_reasoning_level = None;
        }
        AutoTitleModelContext {
            provider_id: per_turn_config.model_provider_id.clone(),
            provider: per_turn_config.model_provider.clone(),
            model_info: title_model_info.clone(),
            instructions,
            session_telemetry: self.services.session_telemetry.clone().with_model(
                selection.model_slug.as_str(),
                title_model_info.slug.as_str(),
            ),
            service_tier: session_configuration.service_tier,
            personality: session_configuration.personality,
            profile: selection.profile,
            reasoning_effort: selection.reasoning_effort,
        }
    }

    /// Returns the current model runtime context for automatic thread summaries.
    pub(crate) async fn auto_summary_model_context(&self) -> AutoSummaryModelContext {
        let session_configuration = {
            let state = self.state.lock().await;
            state.session_configuration.clone()
        };
        let per_turn_config = Self::build_per_turn_config(&session_configuration);
        let product_profile = session_configuration
            .session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product);
        let current_model_slug = session_configuration.collaboration_mode.model().to_string();
        let model_info = self
            .services
            .models_manager
            .get_model_info(current_model_slug.as_str(), &per_turn_config)
            .await;
        let instructions = self.llm_runtime_catalog.resolve_prompt_for_model(
            &model_info,
            per_turn_config.model_provider_id.as_str(),
            &per_turn_config.model_provider,
            product_profile,
            LlmPromptPurpose::AutoSummary,
        );
        AutoSummaryModelContext {
            provider_id: per_turn_config.model_provider_id.clone(),
            provider: per_turn_config.model_provider.clone(),
            model_info: model_info.clone(),
            instructions,
            session_telemetry: self
                .services
                .session_telemetry
                .clone()
                .with_model(current_model_slug.as_str(), model_info.slug.as_str()),
            service_tier: session_configuration.service_tier,
            personality: session_configuration.personality,
        }
    }

    /// Persists a thread name (delegates to the handler).
    pub(crate) async fn apply_thread_name(self: &Arc<Self>, name: String) {
        let Some(name) = crate::util::normalize_thread_name(&name) else {
            return;
        };

        if let Err(err) = praxis_rollout::ThreadNameWriter::new(self.services.state_db.as_deref())
            .write_name(self.conversation_id, &name)
            .await
        {
            warn!("failed to apply automatic thread name: {err}");
            return;
        }

        {
            let mut state = self.state.lock().await;
            state.session_configuration.thread_name = Some(name.clone());
        }

        self.send_event_raw(Event {
            id: self.next_internal_sub_id(),
            msg: EventMsg::ThreadNameUpdated(ThreadNameUpdatedEvent {
                thread_id: self.conversation_id,
                thread_name: Some(name),
            }),
        })
        .await;
    }

    pub(crate) async fn route_realtime_text_input(self: &Arc<Self>, text: String) {
        handlers::user_input_or_turn(
            self,
            self.next_internal_sub_id(),
            Op::UserInput {
                items: vec![UserInput::Text {
                    text,
                    text_elements: Vec::new(),
                }],
                final_output_json_schema: None,
            },
        )
        .await;
    }

    pub(crate) async fn get_total_token_usage(&self) -> i64 {
        let state = self.state.lock().await;
        state.get_total_token_usage(state.server_reasoning_included())
    }

    pub(crate) async fn get_total_token_usage_breakdown(&self) -> TotalTokenUsageBreakdown {
        let state = self.state.lock().await;
        state.history.get_total_token_usage_breakdown()
    }

    pub(crate) async fn total_token_usage(&self) -> Option<TokenUsage> {
        let state = self.state.lock().await;
        state.token_info().map(|info| info.total_token_usage)
    }

    pub(crate) async fn get_estimated_token_count(
        &self,
        turn_context: &TurnContext,
    ) -> Option<i64> {
        let state = self.state.lock().await;
        state.history.estimate_token_count(turn_context)
    }

    pub(crate) async fn get_base_instructions(&self) -> BaseInstructions {
        let state = self.state.lock().await;
        BaseInstructions {
            text: state.session_configuration.base_instructions.clone(),
        }
    }

    // Merges connector IDs into the session-level explicit connector selection.
    pub(crate) async fn merge_connector_selection(
        &self,
        connector_ids: HashSet<String>,
    ) -> HashSet<String> {
        let mut state = self.state.lock().await;
        state.merge_connector_selection(connector_ids)
    }

    // Returns the connector IDs currently selected for this session.
    pub(crate) async fn get_connector_selection(&self) -> HashSet<String> {
        let state = self.state.lock().await;
        state.get_connector_selection()
    }

    // Clears connector IDs that were accumulated for explicit selection.
    pub(crate) async fn clear_connector_selection(&self) {
        let mut state = self.state.lock().await;
        state.clear_connector_selection();
    }

    pub(super) async fn record_initial_history(&self, conversation_history: InitialHistory) {
        let turn_context = self.new_default_turn().await;
        let is_subagent = {
            let state = self.state.lock().await;
            matches!(
                state.session_configuration.session_source,
                SessionSource::SubAgent(_)
            )
        };
        match conversation_history {
            InitialHistory::New => {
                // Defer initial context insertion until the first real turn starts so
                // turn/start overrides can be merged before we write model-visible context.
                self.set_previous_turn_settings(/*previous_turn_settings*/ None)
                    .await;
            }
            InitialHistory::Resumed(resumed_history) => {
                let rollout_items = resumed_history.history;
                let previous_turn_settings = self
                    .apply_rollout_reconstruction(&turn_context, &rollout_items)
                    .await;

                // If resuming, warn when the last recorded model differs from the current one.
                let curr: &str = turn_context.model_info.slug.as_str();
                if let Some(prev) = previous_turn_settings
                    .as_ref()
                    .map(|settings| settings.model.as_str())
                    .filter(|model| *model != curr)
                {
                    warn!("resuming session with different model: previous={prev}, current={curr}");
                    self.send_event(
                        &turn_context,
                        EventMsg::Warning(WarningEvent {
                            message: format!(
                                "This session was recorded with model `{prev}` but is resuming with `{curr}`. \
                         Consider switching back to `{prev}` as it may affect Praxis performance."
                            ),
                        }),
                    )
                    .await;
                }

                // Seed usage info from the recorded rollout so UIs can show token counts
                // immediately on resume/fork.
                if let Some(info) = Self::last_token_info_from_rollout(&rollout_items) {
                    let mut state = self.state.lock().await;
                    state.set_token_info(Some(info));
                }

                // Defer seeding the session's initial context until the first turn starts so
                // turn/start overrides can be merged before we write to the rollout.
                if !is_subagent {
                    self.flush_rollout().await;
                }
            }
            InitialHistory::Forked(rollout_items) => {
                self.apply_rollout_reconstruction(&turn_context, &rollout_items)
                    .await;

                // Seed usage info from the recorded rollout so UIs can show token counts
                // immediately on resume/fork.
                if let Some(info) = Self::last_token_info_from_rollout(&rollout_items) {
                    let mut state = self.state.lock().await;
                    state.set_token_info(Some(info));
                }

                // If persisting, persist all rollout items as-is (recorder filters)
                if !rollout_items.is_empty() {
                    self.persist_rollout_items(&rollout_items).await;
                }

                // Forked threads should remain file-backed immediately after startup.
                self.ensure_rollout_materialized().await;

                // Flush after seeding history and any persisted rollout copy.
                if !is_subagent {
                    self.flush_rollout().await;
                }
            }
        }
    }

    pub(super) async fn apply_rollout_reconstruction(
        &self,
        turn_context: &TurnContext,
        rollout_items: &[RolloutItem],
    ) -> Option<PreviousTurnSettings> {
        let reconstructed_rollout = self
            .reconstruct_history_from_rollout(turn_context, rollout_items)
            .await;
        let previous_turn_settings = reconstructed_rollout.previous_turn_settings.clone();
        self.replace_history(
            reconstructed_rollout.history,
            reconstructed_rollout.reference_context_item,
        )
        .await;
        self.set_previous_turn_settings(previous_turn_settings.clone())
            .await;
        previous_turn_settings
    }

    pub(super) fn last_token_info_from_rollout(
        rollout_items: &[RolloutItem],
    ) -> Option<TokenUsageInfo> {
        rollout_items.iter().rev().find_map(|item| match item {
            RolloutItem::EventMsg(EventMsg::TokenCount(ev)) => ev.info.clone(),
            _ => None,
        })
    }

    pub(super) async fn previous_turn_settings(&self) -> Option<PreviousTurnSettings> {
        let state = self.state.lock().await;
        state.previous_turn_settings()
    }

    pub(crate) async fn set_previous_turn_settings(
        &self,
        previous_turn_settings: Option<PreviousTurnSettings>,
    ) {
        let mut state = self.state.lock().await;
        state.set_previous_turn_settings(previous_turn_settings);
    }
}

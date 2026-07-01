use super::*;
use praxis_app_core::praxis_model_change_divider_message;

impl App {
    pub(super) async fn rebuild_config_for_cwd(
        &self,
        cwd: PathBuf,
    ) -> Result<(Config, TuiRuntimeConfig)> {
        let mut overrides = self.harness_overrides.clone();
        overrides.cwd = Some(cwd.clone());
        let cwd_display = cwd.display().to_string();
        let config = ConfigBuilder::default()
            .praxis_home(self.config.praxis_home.clone())
            .cli_overrides(self.cli_kv_overrides.clone())
            .harness_overrides(overrides)
            .build()
            .await
            .wrap_err_with(|| format!("Failed to rebuild config for cwd {cwd_display}"))?;
        let tui_config = TuiRuntimeConfig::from_core_config(&config)
            .wrap_err_with(|| format!("Failed to load TUI config for cwd {cwd_display}"))?;
        Ok((config, tui_config))
    }

    pub(super) async fn refresh_in_memory_config_from_disk(&mut self) -> Result<()> {
        let (mut config, tui_config) = self
            .rebuild_config_for_cwd(self.chat_widget.config_ref().cwd.to_path_buf())
            .await?;
        self.apply_runtime_policy_overrides(&mut config);
        self.config = config;
        self.tui_config = tui_config;
        self.chat_widget.sync_plugin_mentions_config(&self.config);
        self.chat_widget.set_tui_config(self.tui_config.clone());
        Ok(())
    }

    pub(super) async fn refresh_in_memory_config_from_disk_best_effort(&mut self, action: &str) {
        if let Err(err) = self.refresh_in_memory_config_from_disk().await {
            tracing::warn!(
                error = %err,
                action,
                "failed to refresh config before thread transition; continuing with current in-memory config"
            );
        }
    }

    pub(super) async fn apply_model_provider_selection(
        &mut self,
        model: String,
        provider_id: String,
        provider: Option<ModelProviderInfo>,
        effort: Option<ReasoningEffortConfig>,
        write_mode: ProviderConfigWriteMode,
        configured_provider_label: Option<String>,
    ) {
        let profile_name = self.active_profile.clone();
        let profile = profile_name.as_deref();
        let provider_for_selection = provider
            .clone()
            .or_else(|| self.config.model_providers.get(&provider_id).cloned());
        let Some(provider_for_selection) = provider_for_selection else {
            self.chat_widget.add_error_message(format!(
                "Cannot select {provider_id}::{model}: provider is not configured."
            ));
            return;
        };

        let force_upsert = matches!(write_mode, ProviderConfigWriteMode::ForceUpsert);
        let should_upsert = force_upsert || !self.config.model_providers.contains_key(&provider_id);
        let mut builder = ConfigEditsBuilder::new(&self.config.praxis_home).with_profile(profile);
        if should_upsert {
            builder = builder.upsert_model_provider(provider_id.as_str(), &provider_for_selection);
        }

        match builder
            .set_model_provider(Some(provider_id.as_str()))
            .set_model(Some(model.as_str()), effort)
            .apply()
            .await
        {
            Ok(()) => {
                if let Err(err) = self.refresh_in_memory_config_from_disk().await {
                    tracing::warn!(
                        error = %err,
                        "failed to refresh in-memory config after model selection"
                    );
                }
                self.config.model_provider_id = provider_id.clone();
                self.config
                    .model_providers
                    .insert(provider_id.clone(), provider_for_selection.clone());
                self.config.model_provider = provider_for_selection.clone();
                self.config.model = Some(model.clone());
                self.chat_widget.set_model_selection(
                    &model,
                    &provider_id,
                    Some(&provider_for_selection),
                );
                self.on_update_reasoning_effort(effort);
                self.chat_widget.submit_op(AppCommand::reload_user_config());
                self.chat_widget
                    .submit_op(AppCommand::override_turn_context(
                        /*cwd*/ None,
                        /*approval_policy*/ None,
                        /*approvals_reviewer*/ None,
                        /*sandbox_policy*/ None,
                        /*windows_sandbox_level*/ None,
                        Some(provider_id.clone()),
                        Some(model.clone()),
                        Some(effort),
                        /*summary*/ None,
                        /*service_tier*/ None,
                        /*collaboration_mode*/ None,
                        /*personality*/ None,
                    ));
                let effort_label = effort
                    .map(|selected_effort| selected_effort.to_string())
                    .unwrap_or_else(|| "default".to_string());
                tracing::info!(
                    "Selected provider: {provider_id}, Selected model: {model}, Selected effort: {effort_label}"
                );
                let profile_context = profile.map(|profile| format!("{profile} profile"));
                let mut message = praxis_model_change_divider_message(
                    model.as_str(),
                    Self::reasoning_label_for(&model, effort),
                    profile_context.as_deref(),
                );
                if let Some(label) = configured_provider_label {
                    message = format!("{label} provider configured; {message}");
                }
                self.chat_widget.add_model_change_message(message);
            }
            Err(err) => {
                tracing::error!(
                    error = %err,
                    "failed to persist model/provider selection"
                );
                if let Some(label) = configured_provider_label {
                    self.chat_widget
                        .add_error_message(format!("Failed to save {label} provider: {err}"));
                } else if let Some(profile) = profile {
                    self.chat_widget.add_error_message(format!(
                        "Failed to save model for profile `{profile}`: {err}"
                    ));
                } else {
                    self.chat_widget
                        .add_error_message(format!("Failed to save default model: {err}"));
                }
            }
        }
    }

    pub(super) async fn rebuild_config_for_resume_or_fallback(
        &mut self,
        current_cwd: &Path,
        resume_cwd: PathBuf,
    ) -> Result<(Config, TuiRuntimeConfig)> {
        match self.rebuild_config_for_cwd(resume_cwd.clone()).await {
            Ok(loaded) => Ok(loaded),
            Err(err) => {
                if crate::cwds_differ(current_cwd, &resume_cwd) {
                    Err(err)
                } else {
                    let resume_cwd_display = resume_cwd.display().to_string();
                    tracing::warn!(
                        error = %err,
                        cwd = %resume_cwd_display,
                        "failed to rebuild config for same-cwd resume; using current in-memory config"
                    );
                    Ok((self.config.clone(), self.tui_config.clone()))
                }
            }
        }
    }

    pub(super) fn apply_runtime_policy_overrides(&mut self, config: &mut Config) {
        if let Some(policy) = self.runtime_approval_policy_override.as_ref()
            && let Err(err) = config.permissions.approval_policy.set(*policy)
        {
            tracing::warn!(%err, "failed to carry forward approval policy override");
            self.chat_widget.add_error_message(format!(
                "Failed to carry forward approval policy override: {err}"
            ));
        }
        if let Some(policy) = self.runtime_sandbox_policy_override.as_ref()
            && let Err(err) = config.permissions.sandbox_policy.set(policy.clone())
        {
            tracing::warn!(%err, "failed to carry forward sandbox policy override");
            self.chat_widget.add_error_message(format!(
                "Failed to carry forward sandbox policy override: {err}"
            ));
        }
    }

    fn current_thread_permissions(
        &self,
    ) -> (
        AskForApproval,
        praxis_protocol::config_types::ApprovalsReviewer,
        SandboxPolicy,
    ) {
        (
            self.config.permissions.approval_policy.value(),
            self.config.approvals_reviewer,
            self.config.permissions.sandbox_policy.get().clone(),
        )
    }

    fn apply_thread_permissions(
        session: &mut ThreadSessionState,
        approval_policy: AskForApproval,
        approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer,
        sandbox_policy: &SandboxPolicy,
    ) {
        session.approval_policy = approval_policy;
        session.approvals_reviewer = approvals_reviewer;
        session.sandbox_policy = sandbox_policy.clone();
    }

    pub(super) fn apply_current_permissions_to_thread_session(
        &self,
        session: &mut ThreadSessionState,
    ) {
        let (approval_policy, approvals_reviewer, sandbox_policy) =
            self.current_thread_permissions();
        Self::apply_thread_permissions(
            session,
            approval_policy,
            approvals_reviewer,
            &sandbox_policy,
        );
    }

    pub(super) fn thread_session_permissions_match_config(
        &self,
        session: &ThreadSessionState,
    ) -> bool {
        let (approval_policy, approvals_reviewer, sandbox_policy) =
            self.current_thread_permissions();
        session.approval_policy == approval_policy
            && session.approvals_reviewer == approvals_reviewer
            && session.sandbox_policy == sandbox_policy
    }

    pub(super) async fn sync_cached_thread_permissions_from_config(&mut self) {
        let (approval_policy, approvals_reviewer, sandbox_policy) =
            self.current_thread_permissions();
        if let Some(session) = self.primary_session_configured.as_mut() {
            Self::apply_thread_permissions(
                session,
                approval_policy,
                approvals_reviewer,
                &sandbox_policy,
            );
        }

        let stores = self
            .thread_event_channels
            .values()
            .map(|channel| Arc::clone(&channel.store))
            .collect::<Vec<_>>();
        for store in stores {
            let mut store = store.lock().await;
            if let Some(session) = store.session.as_mut() {
                Self::apply_thread_permissions(
                    session,
                    approval_policy,
                    approvals_reviewer,
                    &sandbox_policy,
                );
            }
        }
    }
}

use super::*;
use praxis_app_core::praxis_model_change_divider_message;

impl ChatWidget {
    /// Set the approval policy in the widget's config copy.
    pub(crate) fn set_approval_policy(&mut self, policy: AskForApproval) {
        if let Err(err) = self.config.permissions.approval_policy.set(policy) {
            tracing::warn!(%err, "failed to set approval_policy on chat config");
        }
    }

    /// Set the sandbox policy in the widget's config copy.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn set_sandbox_policy(&mut self, policy: SandboxPolicy) -> ConstraintResult<()> {
        self.config.permissions.sandbox_policy.set(policy)?;
        Ok(())
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn set_windows_sandbox_mode(&mut self, mode: Option<WindowsSandboxModeToml>) {
        self.config.permissions.windows_sandbox_mode = mode;
        #[cfg(target_os = "windows")]
        self.bottom_pane.set_windows_degraded_sandbox_active(
            praxis_core::windows_sandbox::ELEVATED_SANDBOX_NUX_ENABLED
                && matches!(
                    WindowsSandboxLevel::from_config(&self.config),
                    WindowsSandboxLevel::RestrictedToken
                ),
        );
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn set_feature_enabled(&mut self, feature: Feature, enabled: bool) -> bool {
        if let Err(err) = self.config.features.set_enabled(feature, enabled) {
            tracing::warn!(
                error = %err,
                feature = feature.key(),
                "failed to update constrained chat widget feature state"
            );
        }
        let enabled = self.config.features.enabled(feature);
        if feature == Feature::RealtimeConversation {
            let realtime_conversation_enabled = self.realtime_conversation_enabled();
            self.bottom_pane
                .set_realtime_conversation_enabled(realtime_conversation_enabled);
            self.bottom_pane
                .set_audio_device_selection_enabled(self.realtime_audio_device_selection_enabled());
            if !realtime_conversation_enabled && self.realtime_conversation.is_live() {
                self.request_realtime_conversation_close(Some(
                    "Realtime voice mode was closed because the feature was disabled.".to_string(),
                ));
            }
        }
        if feature == Feature::FastMode {
            self.sync_fast_command_enabled();
        }
        if feature == Feature::Personality {
            self.sync_personality_command_enabled();
        }
        if feature == Feature::Plugins {
            self.sync_plugins_command_enabled();
            self.refresh_plugin_mentions();
        }
        if feature == Feature::PreventIdleSleep {
            self.turn_sleep_inhibitor = SleepInhibitor::new(enabled);
            self.turn_sleep_inhibitor
                .set_turn_running(self.agent_turn_running);
        }
        enabled
    }

    pub(crate) fn set_approvals_reviewer(&mut self, policy: ApprovalsReviewer) {
        self.config.approvals_reviewer = policy;
    }

    pub(crate) fn set_full_access_warning_acknowledged(&mut self, acknowledged: bool) {
        self.config.notices.hide_full_access_warning = Some(acknowledged);
    }

    pub(crate) fn set_world_writable_warning_acknowledged(&mut self, acknowledged: bool) {
        self.config.notices.hide_world_writable_warning = Some(acknowledged);
    }

    pub(crate) fn set_rate_limit_switch_prompt_hidden(&mut self, hidden: bool) {
        self.config.notices.hide_rate_limit_model_nudge = Some(hidden);
        if hidden {
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Idle;
        }
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn world_writable_warning_hidden(&self) -> bool {
        self.config
            .notices
            .hide_world_writable_warning
            .unwrap_or(false)
    }

    /// Override the reasoning effort used when Plan mode is active.
    ///
    /// When the active mask is already Plan, the override is applied immediately
    /// so the footer reflects it without waiting for the next mode switch.
    /// Passing `None` resets to the Plan-mode preset default.
    pub(crate) fn set_plan_mode_reasoning_effort(&mut self, effort: Option<ReasoningEffortConfig>) {
        self.config.plan_mode_reasoning_effort = effort.clone();
        if self.collaboration_modes_enabled()
            && let Some(mask) = self.active_collaboration_mask.as_mut()
            && mask.mode == Some(ModeKind::Plan)
        {
            if let Some(effort) = effort {
                mask.reasoning_effort = Some(Some(effort));
            } else if let Some(plan_mask) =
                collaboration_modes::plan_mask(self.model_catalog.as_ref())
            {
                mask.reasoning_effort = plan_mask.reasoning_effort;
            }
        }
        self.refresh_model_dependent_surfaces();
    }

    /// Set the reasoning effort for the non-Plan collaboration mode.
    ///
    /// Does not touch the active Plan mask — Plan reasoning is controlled
    /// exclusively by the Plan preset and `set_plan_mode_reasoning_effort`.
    pub(crate) fn set_reasoning_effort(&mut self, effort: Option<ReasoningEffortConfig>) {
        self.current_collaboration_mode = self.current_collaboration_mode.with_updates(
            /*model*/ None,
            Some(effort.clone()),
            /*developer_instructions*/ None,
        );
        if self.collaboration_modes_enabled()
            && let Some(mask) = self.active_collaboration_mask.as_mut()
            && mask.mode != Some(ModeKind::Plan)
        {
            // Generic "global default" updates should not mutate the active Plan mask.
            // Plan reasoning is controlled by the Plan preset and Plan-only override updates.
            mask.reasoning_effort = Some(effort);
        }
        self.refresh_model_dependent_surfaces();
    }

    /// Set the personality in the widget's config copy.
    pub(crate) fn set_personality(&mut self, personality: Personality) {
        self.config.personality = Some(personality);
    }

    /// Set Fast mode in the widget's config copy.
    pub(crate) fn set_service_tier(&mut self, service_tier: Option<ServiceTier>) {
        self.config.service_tier = service_tier;
    }

    pub(crate) fn current_service_tier(&self) -> Option<ServiceTier> {
        self.config.service_tier
    }

    pub(crate) fn status_account_display(&self) -> Option<&StatusAccountDisplay> {
        self.status_account_display.as_ref()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn model_catalog(&self) -> Arc<ModelCatalog> {
        self.model_catalog.clone()
    }

    pub(crate) fn current_plan_type(&self) -> Option<PlanType> {
        self.plan_type
    }

    pub(crate) fn has_chatgpt_account(&self) -> bool {
        self.has_chatgpt_account
    }

    pub(crate) fn update_account_state(
        &mut self,
        status_account_display: Option<StatusAccountDisplay>,
        plan_type: Option<PlanType>,
        has_chatgpt_account: bool,
    ) {
        self.status_account_display = status_account_display;
        self.plan_type = plan_type;
        self.has_chatgpt_account = has_chatgpt_account;
        self.bottom_pane
            .set_connectors_enabled(self.connectors_enabled());
    }

    pub(crate) fn should_show_fast_status(
        &self,
        model: &str,
        service_tier: Option<ServiceTier>,
    ) -> bool {
        model == FAST_STATUS_MODEL
            && matches!(service_tier, Some(ServiceTier::Fast))
            && self.has_chatgpt_account
    }

    pub(super) fn fast_mode_enabled(&self) -> bool {
        self.config.features.enabled(Feature::FastMode)
    }

    pub(crate) fn set_realtime_audio_device(
        &mut self,
        kind: RealtimeAudioDeviceKind,
        name: Option<String>,
    ) {
        match kind {
            RealtimeAudioDeviceKind::Microphone => self.config.realtime_audio.microphone = name,
            RealtimeAudioDeviceKind::Speaker => self.config.realtime_audio.speaker = name,
        }
    }

    /// Set the syntax theme override in the widget's config copy.
    pub(crate) fn set_tui_theme(&mut self, theme: Option<String>) {
        self.tui_config.theme = theme;
    }

    /// Set the model in the widget's config copy and stored collaboration mode.
    pub(crate) fn set_model(&mut self, model: &str) {
        self.current_collaboration_mode = self.current_collaboration_mode.with_updates(
            Some(model.to_string()),
            /*effort*/ None,
            /*developer_instructions*/ None,
        );
        if self.collaboration_modes_enabled()
            && let Some(mask) = self.active_collaboration_mask.as_mut()
        {
            mask.model = Some(model.to_string());
        }
        self.refresh_model_dependent_surfaces();
    }

    pub(crate) fn set_model_selection(
        &mut self,
        model: &str,
        provider_id: &str,
        provider: Option<&praxis_core::ModelProviderInfo>,
    ) {
        self.set_model(model);
        self.config.model_provider_id = provider_id.to_owned();
        if let Some(provider) = provider {
            self.config.model_provider = provider.clone();
            self.config
                .model_providers
                .entry(provider_id.to_owned())
                .or_insert_with(|| provider.clone());
        }
    }

    pub(super) fn set_service_tier_selection(&mut self, service_tier: Option<ServiceTier>) {
        self.set_service_tier(service_tier);
        self.app_event_tx.send(AppEvent::AgentOp(
            AppCommand::override_turn_context(
                /*cwd*/ None,
                /*approval_policy*/ None,
                /*approvals_reviewer*/ None,
                /*sandbox_policy*/ None,
                /*windows_sandbox_level*/ None,
                /*model_provider*/ None,
                /*model*/ None,
                /*effort*/ None,
                /*summary*/ None,
                Some(service_tier),
                /*collaboration_mode*/ None,
                /*personality*/ None,
            )
            .into_core(),
        ));
        self.app_event_tx
            .send(AppEvent::PersistServiceTierSelection { service_tier });
    }

    pub(crate) fn current_model(&self) -> &str {
        if !self.collaboration_modes_enabled() {
            return self.current_collaboration_mode.model();
        }
        self.active_collaboration_mask
            .as_ref()
            .and_then(|mask| mask.model.as_deref())
            .unwrap_or_else(|| self.current_collaboration_mode.model())
    }

    pub(crate) fn current_model_provider_id(&self) -> &str {
        self.config.model_provider_id.as_str()
    }

    pub(crate) fn workspace_theme(&self) -> workspace_theme::WorkspaceTheme {
        workspace_theme::for_preference(
            self.tui_config.surface_theme.as_deref(),
            self.current_model_provider_id(),
            self.model_display_name(),
        )
    }

    pub(super) fn workspace_theme_kind(&self) -> workspace_theme::WorkspaceThemeKind {
        workspace_theme::kind_for_preference(
            self.tui_config.surface_theme.as_deref(),
            self.current_model_provider_id(),
            self.model_display_name(),
        )
    }

    pub(super) fn sync_surface_theme(&mut self) {
        let theme = self.workspace_theme();
        crate::surface::set_runtime_theme_kind(theme.kind);
        self.bottom_pane.set_surface_theme(theme);
    }

    pub(crate) fn realtime_conversation_is_live(&self) -> bool {
        self.realtime_conversation.is_live()
    }

    pub(super) fn current_realtime_audio_device_name(
        &self,
        kind: RealtimeAudioDeviceKind,
    ) -> Option<String> {
        match kind {
            RealtimeAudioDeviceKind::Microphone => self.config.realtime_audio.microphone.clone(),
            RealtimeAudioDeviceKind::Speaker => self.config.realtime_audio.speaker.clone(),
        }
    }

    pub(super) fn current_realtime_audio_selection_label(
        &self,
        kind: RealtimeAudioDeviceKind,
    ) -> String {
        self.current_realtime_audio_device_name(kind)
            .unwrap_or_else(|| "System default".to_string())
    }

    pub(super) fn sync_fast_command_enabled(&mut self) {
        self.bottom_pane
            .set_fast_command_enabled(self.fast_mode_enabled());
    }

    pub(super) fn sync_personality_command_enabled(&mut self) {
        self.bottom_pane
            .set_personality_command_enabled(self.config.features.enabled(Feature::Personality));
    }

    pub(super) fn sync_plugins_command_enabled(&mut self) {
        self.bottom_pane
            .set_plugins_command_enabled(self.config.features.enabled(Feature::Plugins));
    }

    pub(super) fn current_model_supports_personality(&self) -> bool {
        let model = self.current_model();
        self.model_catalog
            .try_list_models()
            .ok()
            .and_then(|models| {
                models
                    .into_iter()
                    .find(|preset| preset.model == model)
                    .map(|preset| preset.supports_personality)
            })
            .unwrap_or(false)
    }

    /// Return whether the effective model currently advertises image-input support.
    ///
    /// We intentionally default to `true` when model metadata cannot be read so transient catalog
    /// failures do not hard-block user input in the UI.
    pub(super) fn current_model_supports_images(&self) -> bool {
        let model = self.current_model();
        self.model_catalog
            .try_list_models()
            .ok()
            .and_then(|models| {
                models
                    .into_iter()
                    .find(|preset| preset.model == model)
                    .map(|preset| preset.input_modalities.contains(&InputModality::Image))
            })
            .unwrap_or(true)
    }

    pub(super) fn sync_image_paste_enabled(&mut self) {
        let enabled = self.current_model_supports_images();
        self.bottom_pane.set_image_paste_enabled(enabled);
    }

    pub(super) fn image_inputs_not_supported_message(&self) -> String {
        format!(
            "Model {} does not support image inputs. Remove images or switch models.",
            self.current_model()
        )
    }

    #[allow(dead_code)] // Used in tests
    pub(crate) fn current_collaboration_mode(&self) -> &CollaborationMode {
        &self.current_collaboration_mode
    }

    pub(crate) fn current_reasoning_effort(&self) -> Option<ReasoningEffortConfig> {
        self.effective_reasoning_effort()
    }

    #[cfg(test)]
    pub(crate) fn active_collaboration_mode_kind(&self) -> ModeKind {
        self.active_mode_kind()
    }

    pub(super) fn is_session_configured(&self) -> bool {
        self.thread_id.is_some()
    }

    pub(super) fn collaboration_modes_enabled(&self) -> bool {
        true
    }

    pub(super) fn initial_collaboration_mask(
        _config: &Config,
        model_catalog: &ModelCatalog,
        model_override: Option<&str>,
    ) -> Option<CollaborationModeMask> {
        let mut mask = collaboration_modes::default_mask(model_catalog)?;
        if let Some(model_override) = model_override {
            mask.model = Some(model_override.to_string());
        }
        Some(mask)
    }

    pub(super) fn active_mode_kind(&self) -> ModeKind {
        self.active_collaboration_mask
            .as_ref()
            .and_then(|mask| mask.mode)
            .unwrap_or(ModeKind::Default)
    }

    pub(super) fn effective_reasoning_effort(&self) -> Option<ReasoningEffortConfig> {
        if !self.collaboration_modes_enabled() {
            return self.current_collaboration_mode.reasoning_effort();
        }
        let current_effort = self.current_collaboration_mode.reasoning_effort();
        self.active_collaboration_mask
            .as_ref()
            .and_then(|mask| mask.reasoning_effort.clone())
            .unwrap_or(current_effort)
    }

    pub(super) fn effective_collaboration_mode(&self) -> CollaborationMode {
        if !self.collaboration_modes_enabled() {
            return self.current_collaboration_mode.clone();
        }
        self.active_collaboration_mask.as_ref().map_or_else(
            || self.current_collaboration_mode.clone(),
            |mask| self.current_collaboration_mode.apply_mask(mask),
        )
    }

    pub(super) fn refresh_model_display(&mut self) {
        let effective = self.effective_collaboration_mode();
        self.session_header.set_model(effective.model());
        // Keep composer paste affordances aligned with the currently effective model.
        self.sync_image_paste_enabled();
        self.refresh_terminal_title();
    }

    /// Refresh every UI surface that depends on the effective model, reasoning
    /// effort, or collaboration mode.
    ///
    /// Call this at the end of any setter that mutates `current_collaboration_mode`,
    /// `active_collaboration_mask`, or per-mode reasoning-effort overrides.
    /// Consolidating both refreshes here prevents the bug where callers update the
    /// header/title (`refresh_model_display`) but forget the footer status line
    /// (`refresh_status_line`).
    pub(super) fn refresh_model_dependent_surfaces(&mut self) {
        self.sync_surface_theme();
        self.refresh_model_display();
        self.refresh_status_line();
    }

    pub(super) fn model_display_name(&self) -> &str {
        let model = self.current_model();
        if model.is_empty() {
            DEFAULT_MODEL_DISPLAY_NAME
        } else {
            model
        }
    }

    /// Get the label for the current collaboration mode.
    pub(super) fn collaboration_mode_label(&self) -> Option<&'static str> {
        if !self.collaboration_modes_enabled() {
            return None;
        }
        let active_mode = self.active_mode_kind();
        active_mode
            .is_tui_visible()
            .then_some(active_mode.display_name())
    }

    pub(super) fn collaboration_mode_indicator(&self) -> Option<CollaborationModeIndicator> {
        if !self.collaboration_modes_enabled() {
            return None;
        }
        match self.active_mode_kind() {
            ModeKind::Plan => Some(CollaborationModeIndicator::Plan),
            ModeKind::Default | ModeKind::PairProgramming | ModeKind::Execute => None,
        }
    }

    pub(super) fn update_collaboration_mode_indicator(&mut self) {
        let indicator = self.collaboration_mode_indicator();
        self.bottom_pane.set_collaboration_mode_indicator(indicator);
    }

    pub(super) fn personality_label(personality: Personality) -> &'static str {
        match personality {
            Personality::None => "None",
            Personality::Friendly => "Friendly",
            Personality::Pragmatic => "Pragmatic",
        }
    }

    pub(super) fn personality_description(personality: Personality) -> &'static str {
        match personality {
            Personality::None => "No personality instructions.",
            Personality::Friendly => "Warm, collaborative, and helpful.",
            Personality::Pragmatic => "Concise, task-focused, and direct.",
        }
    }

    /// Cycle to the next collaboration mode variant (Plan -> Default -> Plan).
    pub(super) fn cycle_collaboration_mode(&mut self) {
        if !self.collaboration_modes_enabled() {
            return;
        }

        if let Some(next_mask) = collaboration_modes::next_mask(
            self.model_catalog.as_ref(),
            self.active_collaboration_mask.as_ref(),
        ) {
            self.set_collaboration_mask(next_mask);
        }
    }

    /// Update the active collaboration mask.
    ///
    /// When collaboration modes are enabled and a preset is selected,
    /// the current mode is attached to submissions as `Op::UserTurn { collaboration_mode: Some(...) }`.
    pub(crate) fn set_collaboration_mask(&mut self, mut mask: CollaborationModeMask) {
        if !self.collaboration_modes_enabled() {
            return;
        }
        let previous_mode = self.active_mode_kind();
        let previous_model = self.current_model().to_string();
        let previous_effort = self.effective_reasoning_effort();
        if mask.mode == Some(ModeKind::Plan)
            && let Some(effort) = self.config.plan_mode_reasoning_effort.clone()
        {
            mask.reasoning_effort = Some(Some(effort));
        }
        self.active_collaboration_mask = Some(mask);
        self.update_collaboration_mode_indicator();
        self.refresh_model_dependent_surfaces();
        let next_mode = self.active_mode_kind();
        let next_model = self.current_model();
        let next_effort = self.effective_reasoning_effort();
        if previous_mode != next_mode
            && (previous_model != next_model || previous_effort != next_effort)
        {
            let context = format!("{} mode", next_mode.display_name());
            let message = praxis_model_change_divider_message(
                next_model,
                Some(Self::status_line_reasoning_effort_label(next_effort)),
                Some(context.as_str()),
            );
            self.add_model_change_message(message);
        }
        self.request_redraw();
    }
}

impl BottomPane {
    pub fn new(params: BottomPaneParams) -> Self {
        let BottomPaneParams {
            app_event_tx,
            frame_requester,
            has_input_focus,
            enhanced_keys_supported,
            placeholder_text,
            disable_paste_burst,
            animations_enabled,
            skills,
        } = params;
        let mut composer = ChatComposer::new(
            has_input_focus,
            app_event_tx.clone(),
            enhanced_keys_supported,
            placeholder_text,
            disable_paste_burst,
        );
        composer.set_frame_requester(frame_requester.clone());
        composer.set_skill_mentions(skills);
        Self {
            composer,
            view_stack: Vec::new(),
            app_event_tx,
            frame_requester,
            has_input_focus,
            enhanced_keys_supported,
            disable_paste_burst,
            is_task_running: false,
            status: None,
            status_activity_message: None,
            status_footer_message: None,
            unified_exec_footer: UnifiedExecFooter::new(),
            pending_input_preview: PendingInputPreview::new(),
            pending_thread_approvals: PendingThreadApprovals::new(),
            esc_backtrack_hint: false,
            animations_enabled,
            context_window_percent: None,
            context_window_used_tokens: None,
        }
    }

    pub fn set_skills(&mut self, skills: Option<Vec<SkillMetadata>>) {
        self.composer.set_skill_mentions(skills);
        self.request_redraw();
    }

    pub fn set_animations_enabled(&mut self, enabled: bool) {
        self.animations_enabled = enabled;
        self.request_redraw();
    }

    pub(crate) fn set_surface_theme(&mut self, theme: SurfaceTheme) {
        self.composer.set_surface_theme(theme);
        self.request_redraw();
    }

    /// Update image-paste behavior for the active composer and repaint immediately.
    ///
    /// Callers use this to keep composer affordances aligned with model capabilities.
    pub fn set_image_paste_enabled(&mut self, enabled: bool) {
        self.composer.set_image_paste_enabled(enabled);
        self.request_redraw();
    }

    pub fn set_connectors_snapshot(&mut self, snapshot: Option<ConnectorsSnapshot>) {
        self.composer.set_connector_mentions(snapshot);
        self.request_redraw();
    }

    pub fn set_plugin_mentions(&mut self, plugins: Option<Vec<PluginCapabilitySummary>>) {
        self.composer.set_plugin_mentions(plugins);
        self.request_redraw();
    }

    pub fn set_plugins_command_enabled(&mut self, enabled: bool) {
        self.composer.set_plugins_command_enabled(enabled);
        self.request_redraw();
    }

    pub fn take_mention_bindings(&mut self) -> Vec<MentionBinding> {
        self.composer.take_mention_bindings()
    }

    pub fn take_recent_submission_mention_bindings(&mut self) -> Vec<MentionBinding> {
        self.composer.take_recent_submission_mention_bindings()
    }

    /// Clear pending attachments and mention bindings e.g. when a slash command doesn't submit text.
    pub(crate) fn drain_pending_submission_state(&mut self) {
        let _ = self.take_recent_submission_images_with_placeholders();
        let _ = self.take_remote_image_urls();
        let _ = self.take_recent_submission_mention_bindings();
        let _ = self.take_mention_bindings();
    }

    pub fn set_collaboration_modes_enabled(&mut self, enabled: bool) {
        self.composer.set_collaboration_modes_enabled(enabled);
        self.request_redraw();
    }

    pub fn set_connectors_enabled(&mut self, enabled: bool) {
        self.composer.set_connectors_enabled(enabled);
    }

    #[cfg(target_os = "windows")]
    pub fn set_windows_degraded_sandbox_active(&mut self, enabled: bool) {
        self.composer.set_windows_degraded_sandbox_active(enabled);
        self.request_redraw();
    }

    pub fn set_collaboration_mode_indicator(
        &mut self,
        indicator: Option<CollaborationModeIndicator>,
    ) {
        self.composer.set_collaboration_mode_indicator(indicator);
        self.request_redraw();
    }

    pub fn set_personality_command_enabled(&mut self, enabled: bool) {
        self.composer.set_personality_command_enabled(enabled);
        self.request_redraw();
    }

    pub fn set_fast_command_enabled(&mut self, enabled: bool) {
        self.composer.set_fast_command_enabled(enabled);
        self.request_redraw();
    }

    pub fn set_realtime_conversation_enabled(&mut self, enabled: bool) {
        self.composer.set_realtime_conversation_enabled(enabled);
        self.request_redraw();
    }

    pub fn set_audio_device_selection_enabled(&mut self, enabled: bool) {
        self.composer.set_audio_device_selection_enabled(enabled);
        self.request_redraw();
    }

    /// Update the key hint shown next to queued messages so it matches the
    /// binding that `ChatWidget` actually listens for.
    pub(crate) fn set_queued_message_edit_binding(&mut self, binding: KeyBinding) {
        self.pending_input_preview.set_edit_binding(binding);
        self.request_redraw();
    }

    pub fn status_widget(&self) -> Option<&StatusIndicatorWidget> {
        self.status.as_ref()
    }

    pub(crate) fn status_widget_mut(&mut self) -> Option<&mut StatusIndicatorWidget> {
        self.status.as_mut()
    }

    pub fn skills(&self) -> Option<&Vec<SkillMetadata>> {
        self.composer.skills()
    }

    pub fn plugins(&self) -> Option<&Vec<PluginCapabilitySummary>> {
        self.composer.plugins()
    }

    #[cfg(test)]
    pub(crate) fn context_window_percent(&self) -> Option<i64> {
        self.context_window_percent
    }

    #[cfg(test)]
    pub(crate) fn context_window_used_tokens(&self) -> Option<i64> {
        self.context_window_used_tokens
    }

    fn active_view(&self) -> Option<&dyn BottomPaneView> {
        self.view_stack.last().map(std::convert::AsRef::as_ref)
    }

    fn push_view(&mut self, view: Box<dyn BottomPaneView>) {
        self.view_stack.push(view);
        self.request_redraw();
    }
}

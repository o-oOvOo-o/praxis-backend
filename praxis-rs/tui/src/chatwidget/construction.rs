use super::*;

impl ChatWidget {
    pub(crate) fn new_with_app_event(common: ChatWidgetInit) -> Self {
        Self::new_with_op_target(common, PraxisOpTarget::AppEvent)
    }

    #[allow(dead_code)]
    pub(crate) fn new_with_op_sender(
        common: ChatWidgetInit,
        praxis_op_tx: UnboundedSender<Op>,
    ) -> Self {
        Self::new_with_op_target(common, PraxisOpTarget::Direct(praxis_op_tx))
    }

    fn new_with_op_target(common: ChatWidgetInit, praxis_op_target: PraxisOpTarget) -> Self {
        let ChatWidgetInit {
            config,
            tui_config,
            frame_requester,
            app_event_tx,
            initial_user_message,
            enhanced_keys_supported,
            has_chatgpt_account,
            model_catalog,
            feedback,
            is_first_run,
            status_account_display,
            initial_plan_type,
            model,
            startup_tooltip_override,
            status_line_invalid_items_warned,
            terminal_title_invalid_items_warned,
            session_telemetry,
        } = common;
        let model = model.filter(|m| !m.trim().is_empty());
        let mut config = config;
        config.model = model.clone();
        let prevent_idle_sleep = config.features.enabled(Feature::PreventIdleSleep);
        let placeholder = DEFAULT_COMPOSER_PLACEHOLDER.to_string();

        let model_override = model.as_deref();
        let model_for_header = model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL_DISPLAY_NAME.to_string());
        let active_collaboration_mask =
            Self::initial_collaboration_mask(&config, model_catalog.as_ref(), model_override);
        let header_model = active_collaboration_mask
            .as_ref()
            .and_then(|mask| mask.model.clone())
            .unwrap_or_else(|| model_for_header.clone());
        let fallback_default = Settings {
            model: header_model.clone(),
            reasoning_effort: None,
            developer_instructions: None,
        };
        // Collaboration modes start in Default mode.
        let current_collaboration_mode = CollaborationMode {
            mode: ModeKind::Default,
            settings: fallback_default,
        };

        let active_cell = None;

        let current_cwd = Some(config.cwd.to_path_buf());
        let queued_message_edit_binding = queued_message_edit_binding_for_terminal(terminal_info());
        let mut widget = Self {
            app_event_tx: app_event_tx.clone(),
            frame_requester: frame_requester.clone(),
            praxis_op_target,
            bottom_pane: BottomPane::new(BottomPaneParams {
                frame_requester,
                app_event_tx,
                has_input_focus: true,
                enhanced_keys_supported,
                placeholder_text: placeholder,
                disable_paste_burst: config.disable_paste_burst,
                animations_enabled: tui_config.animations,
                skills: None,
            }),
            active_cell,
            active_cell_revision: 0,
            transcript_search: TranscriptSearchState::default(),
            transcript_search_document_cache: None,
            config,
            tui_config,
            ui_language: UiLanguage::default(),
            skills_all: Vec::new(),
            skills_initial_state: None,
            current_collaboration_mode,
            active_collaboration_mask,
            has_chatgpt_account,
            model_catalog,
            session_telemetry,
            session_header: SessionHeader::new(header_model),
            initial_user_message,
            status_account_display,
            token_info: None,
            thread_control_state: None,
            rate_limit_snapshots_by_limit_id: BTreeMap::new(),
            refreshing_status_outputs: Vec::new(),
            next_status_refresh_request_id: 0,
            plan_type: initial_plan_type,
            rate_limit_warnings: RateLimitWarningState::default(),
            rate_limit_switch_prompt: RateLimitSwitchPromptState::default(),
            adaptive_chunking: AdaptiveChunkingPolicy::default(),
            stream_controller: None,
            plan_stream_controller: None,
            last_copyable_output: None,
            running_commands: HashMap::new(),
            collab_agent_metadata: HashMap::new(),
            pending_collab_spawn_requests: HashMap::new(),
            suppressed_exec_calls: HashSet::new(),
            last_unified_wait: None,
            unified_exec_wait_streak: None,
            turn_sleep_inhibitor: SleepInhibitor::new(prevent_idle_sleep),
            task_complete_pending: false,
            unified_exec_processes: Vec::new(),
            agent_turn_running: false,
            mcp_startup_status: None,
            pending_turn_copyable_output: None,
            mcp_startup_expected_servers: None,
            mcp_startup_ignore_updates_until_next_start: false,
            mcp_startup_allow_terminal_only_next_round: false,
            mcp_startup_pending_next_round: HashMap::new(),
            mcp_startup_pending_next_round_saw_starting: false,
            connectors_cache: ConnectorsCacheState::default(),
            connectors_partial_snapshot: None,
            connectors_prefetch_in_flight: false,
            connectors_force_refetch_pending: false,
            plugins_cache: PluginsCacheState::default(),
            plugins_fetch_state: PluginListFetchState::default(),
            plugin_install_apps_needing_auth: Vec::new(),
            plugin_install_auth_flow: None,
            interrupts: InterruptManager::new(),
            reasoning_buffer: String::new(),
            full_reasoning_buffer: String::new(),
            reasoning_block_kind: None,
            current_status: StatusIndicatorState::turn_running(),
            turn_status_snapshot: TurnRuntimeState::default(),
            pending_guardian_review_status: PendingGuardianReviewStatus::default(),
            terminal_title_status_kind: TerminalTitleStatusKind::TurnRunning,
            retry_status_header: None,
            pending_status_indicator_restore: false,
            suppress_queue_autosend: false,
            thread_id: None,
            thread_name: None,
            forked_from: None,
            queued_user_messages: VecDeque::new(),
            rejected_steers_queue: VecDeque::new(),
            pending_steers: VecDeque::new(),
            submit_pending_steers_after_interrupt: false,
            pending_thread_approvals_count: 0,
            queued_message_edit_binding,
            show_welcome_banner: is_first_run,
            startup_tooltip_override,
            suppress_session_configured_redraw: false,
            suppress_initial_user_message_submit: false,
            pending_notification: None,
            in_app_toasts: ToastQueue::new(/*capacity*/ 3),
            quit_shortcut_expires_at: None,
            quit_shortcut_key: None,
            is_review_mode: false,
            pre_review_token_info: None,
            needs_final_message_separator: false,
            had_work_activity: false,
            saw_plan_update_this_turn: false,
            saw_plan_item_this_turn: false,
            last_plan_progress: None,
            work_panel: WorkPanelState::default(),
            plan_delta_buffer: String::new(),
            plan_item_active: false,
            last_separator_elapsed_secs: None,
            turn_runtime_metrics: RuntimeMetricsSummary::default(),
            last_rendered_width: Cell::new(None),
            last_visible_patch_cell_ids: RefCell::new(Vec::new()),
            active_cell_render_cache: RefCell::new(None),
            workspace_active_tail_cache: RefCell::new(None),
            workspace_transcript_cache: RefCell::new(WorkspaceTranscriptCache::default()),
            feedback,
            current_rollout_path: None,
            current_cwd,
            selfwork_plan_path: None,
            selfwork_last_plan_digest: None,
            selfwork_stall_count: 0,
            selfwork_turn_in_flight: false,
            session_network_proxy: None,
            status_line_invalid_items_warned,
            terminal_title_invalid_items_warned,
            last_terminal_title: None,
            terminal_title_setup_original_items: None,
            terminal_title_animation_origin: Instant::now(),
            status_line_project_root_name_cache: None,
            status_line_branch: None,
            status_line_branch_cwd: None,
            status_line_branch_pending: false,
            status_line_branch_lookup_complete: false,
            external_editor_state: ExternalEditorState::Closed,
            realtime_conversation: RealtimeConversationUiState::default(),
            last_rendered_user_message_event: None,
            last_non_retry_error: None,
            pending_goal_completion_elapsed: None,
        };

        widget
            .bottom_pane
            .set_realtime_conversation_enabled(widget.realtime_conversation_enabled());
        widget
            .bottom_pane
            .set_audio_device_selection_enabled(widget.realtime_audio_device_selection_enabled());
        widget
            .bottom_pane
            .set_status_line_enabled(!widget.configured_status_line_items().is_empty());
        widget
            .bottom_pane
            .set_collaboration_modes_enabled(/*enabled*/ true);
        widget.sync_fast_command_enabled();
        widget.sync_personality_command_enabled();
        widget.sync_plugins_command_enabled();
        widget
            .bottom_pane
            .set_queued_message_edit_binding(widget.queued_message_edit_binding);
        #[cfg(target_os = "windows")]
        widget.bottom_pane.set_windows_degraded_sandbox_active(
            praxis_core::windows_sandbox::ELEVATED_SANDBOX_NUX_ENABLED
                && matches!(
                    WindowsSandboxLevel::from_config(&widget.config),
                    WindowsSandboxLevel::RestrictedToken
                ),
        );
        widget.update_collaboration_mode_indicator();

        widget
            .bottom_pane
            .set_connectors_enabled(widget.connectors_enabled());
        widget.sync_surface_theme();
        widget.refresh_status_surfaces();

        widget
    }
}

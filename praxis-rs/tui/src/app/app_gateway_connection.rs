use super::*;
use praxis_features::Feature;

impl App {
    pub(super) async fn drain_app_gateway_events(&mut self, app_gateway: &mut AppGatewaySession) {
        for _ in 0..APP_GATEWAY_EVENT_DRAIN_BUDGET {
            let Some(event) = app_gateway.try_next_event() else {
                break;
            };
            self.handle_app_gateway_event(app_gateway, event).await;
        }
    }

    pub(super) fn mark_app_gateway_disconnected(&mut self, message: String) {
        if self.remote_app_gateway_url.is_some() {
            if !self.app_gateway_reconnect_pending {
                self.chat_widget
                    .add_error_message(format!("{message}; reconnecting to Praxis gateway..."));
            }
            self.app_gateway_reconnect_pending = true;
            return;
        }
        self.chat_widget.add_error_message(message.clone());
        self.app_event_tx.send(AppEvent::FatalExitRequest(message));
    }

    pub(super) fn should_retry_app_gateway_reconnect(&self) -> bool {
        self.app_gateway_reconnect_pending
            && self
                .last_app_gateway_reconnect_attempt
                .is_none_or(|last| last.elapsed() >= APP_GATEWAY_RECONNECT_INTERVAL)
    }

    pub(super) fn app_gateway_reconnect_delay(&self) -> Option<Duration> {
        if !self.app_gateway_reconnect_pending {
            return None;
        }
        Some(
            self.last_app_gateway_reconnect_attempt
                .map_or(Duration::ZERO, |last| {
                    APP_GATEWAY_RECONNECT_INTERVAL.saturating_sub(last.elapsed())
                }),
        )
    }

    pub(super) fn is_app_gateway_transport_error(err: &color_eyre::Report) -> bool {
        err.chain().any(|cause| {
            matches!(
                cause.downcast_ref::<TypedRequestError>(),
                Some(TypedRequestError::Transport { .. })
            ) || cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
                matches!(
                    io.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::NotConnected
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::UnexpectedEof
                )
            })
        })
    }

    pub(super) fn handle_app_gateway_loop_error(
        &mut self,
        app_gateway: &AppGatewaySession,
        err: color_eyre::Report,
    ) -> color_eyre::Result<AppRunControl> {
        if app_gateway.is_remote() && Self::is_app_gateway_transport_error(&err) {
            self.mark_app_gateway_disconnected(err.to_string());
            Ok(AppRunControl::Continue)
        } else {
            Err(err)
        }
    }

    pub(super) async fn try_reconnect_app_gateway(
        &mut self,
        app_gateway: &mut AppGatewaySession,
    ) -> bool {
        if !self.should_retry_app_gateway_reconnect() {
            return false;
        }
        let Some(websocket_url) = self.remote_app_gateway_url.clone() else {
            return false;
        };
        self.last_app_gateway_reconnect_attempt = Some(Instant::now());
        match app_gateway
            .reconnect_remote(websocket_url, self.remote_app_gateway_auth_token.clone())
            .await
        {
            Ok(()) => {
                self.app_gateway_reconnect_pending = false;
                self.last_app_gateway_reconnect_attempt = None;
                self.workspace_observed_thread_ids.clear();
                self.chat_widget
                    .add_info_message("Reconnected to Praxis gateway.".to_string(), None);
                self.refresh_workspace_threads(app_gateway, true);
                self.observe_existing_workspace_threads_if_needed(app_gateway)
                    .await;
                true
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to reconnect to Praxis gateway");
                false
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        tui: &mut tui::Tui,
        mut app_gateway: AppGatewaySession,
        mut config: Config,
        mut tui_config: TuiRuntimeConfig,
        cli_kv_overrides: Vec<(String, TomlValue)>,
        harness_overrides: ConfigOverrides,
        active_profile: Option<String>,
        initial_prompt: Option<String>,
        initial_images: Vec<PathBuf>,
        session_selection: SessionSelection,
        feedback: praxis_feedback::PraxisFeedback,
        is_first_run: bool,
        should_prompt_windows_sandbox_nux_at_startup: bool,
        remote_app_gateway_url: Option<String>,
        remote_app_gateway_auth_token: Option<String>,
        workspace_mode: bool,
    ) -> Result<AppExitInfo> {
        use tokio_stream::StreamExt;
        let (app_event_tx, mut app_event_rx) = unbounded_channel();
        let app_event_tx = AppEventSender::new(app_event_tx);
        emit_project_config_warnings(&app_event_tx, &config);
        emit_system_bwrap_warning(&app_event_tx);
        tui.set_notification_method(tui_config.notification_method);

        let harness_overrides =
            normalize_harness_overrides_for_cwd(harness_overrides, &config.cwd)?;
        let bootstrap = app_gateway.bootstrap(&config).await?;
        let mut model = bootstrap.default_model;
        let discovered_model_catalog = build_model_catalog(&config, bootstrap.available_models);
        let available_models = discovered_model_catalog.models;
        let exit_info = handle_model_migration_prompt_if_needed(
            tui,
            &mut config,
            model.as_str(),
            &app_event_tx,
            &available_models,
        )
        .await;
        if let Some(exit_info) = exit_info {
            app_gateway
                .shutdown()
                .await
                .inspect_err(|err| {
                    tracing::warn!("app-gateway shutdown failed: {err}");
                })
                .ok();
            return Ok(exit_info);
        }
        if let Some(updated_model) = config.model.clone() {
            model = updated_model;
        }
        let model_catalog = Arc::new(ModelCatalog::new(
            available_models.clone(),
            discovered_model_catalog.metadata_by_preset_id,
            CollaborationModesConfig {
                default_mode_request_user_input: config
                    .features
                    .enabled(Feature::DefaultModeRequestUserInput),
            },
        ));
        let feedback_audience = bootstrap.feedback_audience;
        let auth_mode = bootstrap.auth_mode;
        let has_chatgpt_account = bootstrap.has_chatgpt_account;
        let status_account_display = bootstrap.status_account_display.clone();
        let initial_plan_type = bootstrap.plan_type;
        let startup_rate_limit_snapshots = bootstrap.rate_limit_snapshots;
        let session_telemetry = SessionTelemetry::new(
            ThreadId::new(),
            model.as_str(),
            model.as_str(),
            /*account_id*/ None,
            bootstrap.account_email.clone(),
            auth_mode,
            praxis_login::default_client::originator().value,
            config.otel.log_user_prompt,
            user_agent(),
            SessionSource::Cli,
        );
        if tui_config
            .status_line
            .as_ref()
            .is_some_and(|cmd| !cmd.is_empty())
        {
            session_telemetry.counter("praxis.status_line", /*inc*/ 1, &[]);
        }

        let status_line_invalid_items_warned = Arc::new(AtomicBool::new(false));
        let terminal_title_invalid_items_warned = Arc::new(AtomicBool::new(false));

        let enhanced_keys_supported = tui.enhanced_keys_supported();
        let wait_for_initial_session_configured =
            Self::should_wait_for_initial_session(&session_selection);
        let defer_empty_workspace_session = workspace_mode
            && matches!(
                session_selection,
                SessionSelection::StartFresh | SessionSelection::Exit
            )
            && initial_prompt.as_deref().is_none_or(str::is_empty)
            && initial_images.is_empty();
        let (mut chat_widget, initial_started_thread) = match session_selection {
            SessionSelection::StartFresh | SessionSelection::Exit => {
                let startup_tooltip_override = prepare_startup_tooltip_override(
                    &mut config,
                    &mut tui_config,
                    &available_models,
                    is_first_run,
                )
                .await;
                let started = if defer_empty_workspace_session {
                    None
                } else {
                    Some(app_gateway.start_thread(&config).await?)
                };
                let init = crate::chatwidget::ChatWidgetInit {
                    config: config.clone(),
                    tui_config: tui_config.clone(),
                    frame_requester: tui.frame_requester(),
                    app_event_tx: app_event_tx.clone(),
                    initial_user_message: crate::chatwidget::create_initial_user_message(
                        initial_prompt.clone(),
                        initial_images.clone(),
                        // CLI prompt args are plain strings, so they don't provide element ranges.
                        Vec::new(),
                    ),
                    enhanced_keys_supported,
                    has_chatgpt_account,
                    model_catalog: model_catalog.clone(),
                    feedback: feedback.clone(),
                    is_first_run,
                    status_account_display: status_account_display.clone(),
                    initial_plan_type,
                    model: Some(model.clone()),
                    startup_tooltip_override,
                    status_line_invalid_items_warned: status_line_invalid_items_warned.clone(),
                    terminal_title_invalid_items_warned: terminal_title_invalid_items_warned
                        .clone(),
                    session_telemetry: session_telemetry.clone(),
                };
                (ChatWidget::new_with_app_event(init), started)
            }
            SessionSelection::Resume(target_session) => {
                let resumed = app_gateway
                    .resume_thread(config.clone(), target_session.thread_id)
                    .await
                    .wrap_err_with(|| {
                        let target_label = target_session.display_label();
                        format!("Failed to resume session from {target_label}")
                    })?;
                let init = crate::chatwidget::ChatWidgetInit {
                    config: config.clone(),
                    tui_config: tui_config.clone(),
                    frame_requester: tui.frame_requester(),
                    app_event_tx: app_event_tx.clone(),
                    initial_user_message: crate::chatwidget::create_initial_user_message(
                        initial_prompt.clone(),
                        initial_images.clone(),
                        // CLI prompt args are plain strings, so they don't provide element ranges.
                        Vec::new(),
                    ),
                    enhanced_keys_supported,
                    has_chatgpt_account,
                    model_catalog: model_catalog.clone(),
                    feedback: feedback.clone(),
                    is_first_run,
                    status_account_display: status_account_display.clone(),
                    initial_plan_type,
                    model: config.model.clone(),
                    startup_tooltip_override: None,
                    status_line_invalid_items_warned: status_line_invalid_items_warned.clone(),
                    terminal_title_invalid_items_warned: terminal_title_invalid_items_warned
                        .clone(),
                    session_telemetry: session_telemetry.clone(),
                };
                (ChatWidget::new_with_app_event(init), Some(resumed))
            }
            SessionSelection::Fork(target_session) => {
                session_telemetry.counter(
                    "praxis.thread.fork",
                    /*inc*/ 1,
                    &[("source", "cli_subcommand")],
                );
                let mut forked = app_gateway
                    .fork_thread(
                        config.clone(),
                        target_session.thread_id,
                        target_session.path.clone(),
                    )
                    .await
                    .wrap_err_with(|| {
                        let target_label = target_session.display_label();
                        format!("Failed to fork session from {target_label}")
                    })?;
                if forked.session.thread_name.is_none()
                    && let Some(source_name) = target_session.thread_name.as_deref()
                {
                    match app_gateway
                        .thread_set_name(forked.session.thread_id, source_name.to_string())
                        .await
                    {
                        Ok(()) => {
                            forked.session.thread_name = Some(source_name.to_string());
                        }
                        Err(err) => {
                            tracing::warn!(
                                thread_id = %forked.session.thread_id,
                                %err,
                                "Failed to preserve source thread name on fork"
                            );
                        }
                    }
                }
                let init = crate::chatwidget::ChatWidgetInit {
                    config: config.clone(),
                    tui_config: tui_config.clone(),
                    frame_requester: tui.frame_requester(),
                    app_event_tx: app_event_tx.clone(),
                    initial_user_message: crate::chatwidget::create_initial_user_message(
                        initial_prompt.clone(),
                        initial_images.clone(),
                        // CLI prompt args are plain strings, so they don't provide element ranges.
                        Vec::new(),
                    ),
                    enhanced_keys_supported,
                    has_chatgpt_account,
                    model_catalog: model_catalog.clone(),
                    feedback: feedback.clone(),
                    is_first_run,
                    status_account_display: status_account_display.clone(),
                    initial_plan_type,
                    model: config.model.clone(),
                    startup_tooltip_override: None,
                    status_line_invalid_items_warned: status_line_invalid_items_warned.clone(),
                    terminal_title_invalid_items_warned: terminal_title_invalid_items_warned
                        .clone(),
                    session_telemetry: session_telemetry.clone(),
                };
                (ChatWidget::new_with_app_event(init), Some(forked))
            }
        };

        for snapshot in startup_rate_limit_snapshots {
            chat_widget.on_rate_limit_snapshot(Some(snapshot));
        }
        chat_widget
            .maybe_prompt_windows_sandbox_enable(should_prompt_windows_sandbox_nux_at_startup);

        let file_search = FileSearchManager::new(config.cwd.to_path_buf(), app_event_tx.clone());
        #[cfg(not(debug_assertions))]
        let upgrade_version = crate::updates::get_upgrade_version(&config);

        let mut app = Self {
            model_catalog,
            session_telemetry: session_telemetry.clone(),
            app_event_tx,
            chat_widget,
            config,
            tui_config,
            active_profile,
            cli_kv_overrides,
            harness_overrides,
            runtime_approval_policy_override: None,
            runtime_sandbox_policy_override: None,
            file_search,
            enhanced_keys_supported,
            transcript_cells: Vec::new(),
            overlay: None,
            deferred_history_lines: Vec::new(),
            has_emitted_history_lines: false,
            commit_anim_running: Arc::new(AtomicBool::new(false)),
            status_line_invalid_items_warned: status_line_invalid_items_warned.clone(),
            terminal_title_invalid_items_warned: terminal_title_invalid_items_warned.clone(),
            backtrack: BacktrackState::default(),
            backtrack_render_pending: false,
            transcript_scrollback_backfill: None,
            feedback: feedback.clone(),
            feedback_audience,
            remote_app_gateway_url,
            remote_app_gateway_auth_token,
            app_gateway_reconnect_pending: false,
            last_app_gateway_reconnect_attempt: None,
            pending_update_action: None,
            pending_shutdown_exit_thread_id: None,
            windows_sandbox: WindowsSandboxState::default(),
            thread_event_channels: HashMap::new(),
            thread_event_listener_tasks: HashMap::new(),
            agent_navigation: AgentNavigationState::default(),
            active_thread_id: None,
            active_thread_rx: None,
            primary_thread_id: None,
            last_subagent_backfill_attempt: None,
            primary_session_configured: None,
            pending_primary_events: VecDeque::new(),
            pending_app_gateway_requests: PendingAppGatewayRequests::default(),
            workspace: WorkspaceState::new(workspace_mode),
            workspace_observed_thread_ids: HashSet::new(),
            mouse: MouseInteractionState::default(),
            mouse_capture_resume_at: None,
        };
        if let Some(started) = initial_started_thread {
            app.enqueue_primary_thread_session(started.session, started.turns)
                .await?;
        }
        app.refresh_workspace_threads(&app_gateway, true);

        // On startup, if Agent mode (workspace-write) or ReadOnly is active, warn about world-writable dirs on Windows.
        #[cfg(target_os = "windows")]
        {
            let should_check = WindowsSandboxLevel::from_config(&app.config)
                != WindowsSandboxLevel::Disabled
                && matches!(
                    app.config.permissions.sandbox_policy.get(),
                    praxis_protocol::protocol::SandboxPolicy::WorkspaceWrite { .. }
                        | praxis_protocol::protocol::SandboxPolicy::ReadOnly { .. }
                )
                && !app
                    .config
                    .notices
                    .hide_world_writable_warning
                    .unwrap_or(false);
            if should_check {
                let cwd = app.config.cwd.clone();
                let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
                let tx = app.app_event_tx.clone();
                let logs_base_dir = app.config.praxis_home.clone();
                let sandbox_policy = app.config.permissions.sandbox_policy.get().clone();
                Self::spawn_world_writable_scan(
                    cwd.to_path_buf(),
                    env_map,
                    logs_base_dir,
                    sandbox_policy,
                    tx,
                );
            }
        }

        let tui_events = tui.event_stream();
        tokio::pin!(tui_events);

        tui.frame_requester().schedule_frame();

        let mut listen_for_app_gateway_events = true;
        let mut waiting_for_initial_session_configured = wait_for_initial_session_configured;

        #[cfg(not(debug_assertions))]
        let pre_loop_exit_reason = if let Some(latest_version) = upgrade_version {
            let control = app
                .handle_event(
                    tui,
                    &mut app_gateway,
                    AppEvent::InsertHistoryCell(Box::new(UpdateAvailableHistoryCell::new(
                        latest_version,
                        crate::update_action::get_update_action(),
                    ))),
                )
                .await?;
            match control {
                AppRunControl::Continue => None,
                AppRunControl::Exit(exit_reason) => Some(exit_reason),
            }
        } else {
            None
        };
        #[cfg(debug_assertions)]
        let pre_loop_exit_reason: Option<ExitReason> = None;

        let exit_reason_result = if let Some(exit_reason) = pre_loop_exit_reason {
            Ok(exit_reason)
        } else {
            loop {
                let app_gateway_reconnect_delay = app.app_gateway_reconnect_delay();
                let control = select! {
                    _ = tokio::time::sleep(app_gateway_reconnect_delay.unwrap_or(APP_GATEWAY_RECONNECT_INTERVAL)), if app_gateway_reconnect_delay.is_some() => {
                        if app.try_reconnect_app_gateway(&mut app_gateway).await {
                            listen_for_app_gateway_events = true;
                            tui.frame_requester().schedule_frame();
                        }
                        AppRunControl::Continue
                    }
                    Some(event) = app_event_rx.recv() => {
                        match app.handle_event(tui, &mut app_gateway, event).await {
                            Ok(control) => control,
                            Err(err) => match app.handle_app_gateway_loop_error(&app_gateway, err) {
                                Ok(control) => control,
                                Err(err) => break Err(err),
                            },
                        }
                    }
                    active = async {
                        if let Some(rx) = app.active_thread_rx.as_mut() {
                            rx.recv().await
                        } else {
                            None
                        }
                    }, if App::should_handle_active_thread_events(
                        waiting_for_initial_session_configured,
                        app.active_thread_rx.is_some()
                    ) => {
                        if let Some(event) = active {
                            if let Err(err) = app.handle_active_thread_event(tui, &mut app_gateway, event).await {
                                match app.handle_app_gateway_loop_error(&app_gateway, err) {
                                    Ok(control) => control,
                                    Err(err) => break Err(err),
                                }
                            } else {
                                AppRunControl::Continue
                            }
                        } else {
                            app.clear_active_thread().await;
                            AppRunControl::Continue
                        }
                    }
                    Some(event) = tui_events.next() => {
                        match app.handle_tui_event(tui, &mut app_gateway, event).await {
                            Ok(control) => control,
                            Err(err) => match app.handle_app_gateway_loop_error(&app_gateway, err) {
                                Ok(control) => control,
                                Err(err) => break Err(err),
                            },
                        }
                    }
                    app_gateway_event = app_gateway.next_event(), if listen_for_app_gateway_events => {
                        match app_gateway_event {
                            Some(event) => {
                                app.handle_app_gateway_event(&mut app_gateway, event).await;
                                app.drain_app_gateway_events(&mut app_gateway).await;
                                tui.frame_requester().schedule_frame();
                            }
                            None => {
                                listen_for_app_gateway_events = false;
                                if app_gateway.is_remote() {
                                    app.mark_app_gateway_disconnected(
                                        "remote app-gateway event stream closed".to_string(),
                                    );
                                }
                                tracing::warn!("app-gateway event stream closed");
                            }
                        }
                        AppRunControl::Continue
                    }
                };
                if App::should_stop_waiting_for_initial_session(
                    waiting_for_initial_session_configured,
                    app.primary_thread_id,
                ) {
                    waiting_for_initial_session_configured = false;
                }
                match control {
                    AppRunControl::Continue => {}
                    AppRunControl::Exit(reason) => break Ok(reason),
                }
            }
        };
        if let Err(err) = app_gateway.shutdown().await {
            tracing::warn!(error = %err, "failed to shut down embedded app gateway");
        }
        let clear_result = tui.terminal.clear();
        let exit_reason = match exit_reason_result {
            Ok(exit_reason) => {
                clear_result?;
                exit_reason
            }
            Err(err) => {
                if let Err(clear_err) = clear_result {
                    tracing::warn!(error = %clear_err, "failed to clear terminal UI");
                }
                return Err(err);
            }
        };
        Ok(AppExitInfo {
            token_usage: app.token_usage(),
            thread_id: app.chat_widget.thread_id(),
            thread_name: app.chat_widget.thread_name(),
            update_action: app.pending_update_action,
            exit_reason,
        })
    }

    pub(crate) async fn handle_tui_event(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        event: TuiEvent,
    ) -> Result<AppRunControl> {
        self.restore_mouse_capture_after_terminal_zoom(tui);
        if matches!(event, TuiEvent::Draw) {
            let size = tui.terminal.size()?;
            if size != tui.terminal.last_known_screen_size {
                self.refresh_status_line();
            }
        }

        if let TuiEvent::Key(key_event) = event.clone()
            && self.handle_history_presentation_shortcut(tui, key_event)
        {
            return Ok(AppRunControl::Continue);
        }

        if self.overlay.is_some() {
            let _ = self.handle_backtrack_overlay_event(tui, event).await?;
        } else {
            match event {
                TuiEvent::Key(key_event) => {
                    self.handle_key_event(tui, app_gateway, key_event).await;
                }
                TuiEvent::Mouse(mouse_event) => {
                    if let Some(control) = self
                        .handle_mouse_event(tui, app_gateway, mouse_event)
                        .await?
                    {
                        return Ok(control);
                    }
                }
                TuiEvent::Paste(pasted) => {
                    // Many terminals convert newlines to \r when pasting (e.g., iTerm2),
                    // but tui-textarea expects \n. Normalize CR to LF.
                    // [tui-textarea]: https://github.com/rhysd/tui-textarea/blob/4d18622eeac13b309e0ff6a55a46ac6706da68cf/src/textarea.rs#L782-L783
                    // [iTerm2]: https://github.com/gnachman/iTerm2/blob/5d0c0d9f68523cbd0494dad5422998964a2ecd8d/sources/iTermPasteHelper.m#L206-L216
                    let pasted = pasted.replace("\r", "\n");
                    self.chat_widget.handle_paste(pasted);
                }
                TuiEvent::Draw => {
                    self.refresh_workspace_threads(app_gateway, false);
                    if self.backtrack_render_pending {
                        self.start_transcript_scrollback_backfill(tui);
                    }
                    if self.drain_transcript_scrollback_backfill_chunk(tui) {
                        tui.frame_requester().schedule_frame();
                    }
                    self.chat_widget.maybe_post_pending_notification(tui);
                    if self
                        .chat_widget
                        .handle_paste_burst_tick(tui.frame_requester())
                    {
                        return Ok(AppRunControl::Continue);
                    }
                    // Allow widgets to process any pending timers before rendering.
                    self.chat_widget.pre_draw_tick(tui.is_terminal_focused());
                    let terminal_size = tui.terminal.size()?;
                    let draw_height = terminal_size.height;
                    tui.draw(draw_height, |frame| {
                        let chat_area = self.render_workspace_or_chat(frame.area(), frame.buffer);
                        let cursor_pos = if self.workspace.enabled {
                            self.chat_widget.workspace_cursor_pos_embedded(chat_area)
                        } else {
                            self.chat_widget.workspace_cursor_pos(chat_area)
                        };
                        if let Some((x, y)) = cursor_pos {
                            frame.set_cursor_position((x, y));
                        }
                    })?;
                    if self.chat_widget.external_editor_state() == ExternalEditorState::Requested {
                        self.chat_widget
                            .set_external_editor_state(ExternalEditorState::Active);
                        self.app_event_tx.send(AppEvent::LaunchExternalEditor);
                    }
                }
            }
        }
        Ok(AppRunControl::Continue)
    }
}

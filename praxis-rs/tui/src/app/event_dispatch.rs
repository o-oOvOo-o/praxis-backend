use super::*;

impl App {
    pub(super) async fn handle_event(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        event: AppEvent,
    ) -> Result<AppRunControl> {
        if Self::is_provider_policy_event(&event) {
            return self.handle_provider_policy_event(app_gateway, event).await;
        }

        match event {
            AppEvent::NewSession => {
                self.start_fresh_session_with_summary_hint(tui, app_gateway)
                    .await;
            }
            AppEvent::ClearUi => {
                self.clear_terminal_ui(tui, /*redraw_header*/ false)?;
                self.reset_app_ui_state_after_clear();

                self.start_fresh_session_with_summary_hint(tui, app_gateway)
                    .await;
            }
            AppEvent::OpenResumePicker => {
                if let Some(control) = self
                    .open_thread_picker(
                        tui,
                        app_gateway,
                        crate::SessionLookupSource::Praxis,
                        SessionPickerAction::Resume,
                    )
                    .await?
                {
                    return Ok(control);
                }
            }
            AppEvent::OpenThreadPicker { source, action } => {
                if let Some(control) = self
                    .open_thread_picker(tui, app_gateway, source, action)
                    .await?
                {
                    return Ok(control);
                }
            }
            AppEvent::ForkCurrentSession => {
                self.session_telemetry.counter(
                    "praxis.thread.fork",
                    /*inc*/ 1,
                    &[("source", "slash_command")],
                );
                let summary = session_summary(
                    self.chat_widget.token_usage(),
                    self.chat_widget.thread_id(),
                    self.chat_widget.thread_name(),
                );
                self.chat_widget
                    .add_plain_history_lines(vec!["/fork".magenta().into()]);
                if let Some(thread_id) = self.chat_widget.thread_id() {
                    self.refresh_in_memory_config_from_disk_best_effort("forking the thread")
                        .await;
                    match app_gateway
                        .fork_thread(self.config.clone(), thread_id, /*path*/ None)
                        .await
                    {
                        Ok(forked) => {
                            self.shutdown_current_thread(app_gateway).await;
                            match self
                                .replace_chat_widget_with_app_gateway_thread(
                                    tui,
                                    app_gateway,
                                    forked,
                                )
                                .await
                            {
                                Ok(()) => {
                                    if let Some(summary) = summary {
                                        let mut lines: Vec<Line<'static>> =
                                            vec![summary.usage_line.clone().into()];
                                        if let Some(command) = summary.resume_command {
                                            let spans = vec![
                                                "To continue this session, run ".into(),
                                                command.cyan(),
                                            ];
                                            lines.push(spans.into());
                                        }
                                        self.chat_widget.add_plain_history_lines(lines);
                                    }
                                }
                                Err(err) => {
                                    self.chat_widget.add_error_message(format!(
                                        "Failed to attach to forked app-gateway thread: {err}"
                                    ));
                                }
                            }
                        }
                        Err(err) => {
                            self.chat_widget.add_error_message(format!(
                                "Failed to fork current session through the app gateway: {err}"
                            ));
                        }
                    }
                } else {
                    self.chat_widget.add_error_message(
                        "A thread must contain at least one turn before it can be forked."
                            .to_string(),
                    );
                }

                tui.frame_requester().schedule_frame();
            }
            AppEvent::ReleaseCurrentThreadControl => {
                let Some(thread_id) = self.chat_widget.thread_id() else {
                    self.chat_widget.add_info_message(
                        "No active thread is selected.".to_string(),
                        /*hint*/ None,
                    );
                    return Ok(AppRunControl::Continue);
                };
                self.release_workspace_thread_control(app_gateway, thread_id)
                    .await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::RegenerateThreadName { thread_id } => {
                match app_gateway.thread_regenerate_name(thread_id).await {
                    Ok(thread_name) => {
                        if let Some(index) = self.workspace.row_index(thread_id) {
                            self.workspace.rows[index].name = workspace_single_line(&thread_name);
                        }
                        self.chat_widget.add_info_message(
                            format!("Thread name regenerated: {thread_name}"),
                            /*hint*/ None,
                        );
                    }
                    Err(err) => {
                        self.chat_widget
                            .add_error_message(format!("Failed to regenerate thread name: {err}"));
                    }
                }
                tui.frame_requester().schedule_frame();
            }
            AppEvent::OpenThreadGoalMenu { thread_id } => {
                self.open_thread_goal_menu(app_gateway, thread_id).await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::OpenThreadGoalEditor { thread_id } => {
                self.open_thread_goal_editor(app_gateway, thread_id).await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::SetThreadGoalObjective {
                thread_id,
                objective,
                mode,
            } => {
                self.set_thread_goal_objective(app_gateway, thread_id, objective, mode)
                    .await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::SetThreadGoalStatus { thread_id, status } => {
                self.set_thread_goal_status(app_gateway, thread_id, status)
                    .await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::ClearThreadGoal { thread_id } => {
                self.clear_thread_goal(app_gateway, thread_id).await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::InsertHistoryCell(cell) => {
                let cell: Arc<dyn HistoryCell> = cell.into();
                if let Some(Overlay::Transcript(t)) = &mut self.overlay {
                    t.insert_cell(cell.clone());
                    tui.frame_requester().schedule_frame();
                }
                self.transcript_cells.push(cell);
                tui.frame_requester().schedule_frame();
            }
            AppEvent::ApplyThreadRollback { num_turns } => {
                if self.apply_non_pending_thread_rollback(num_turns) {
                    tui.frame_requester().schedule_frame();
                }
            }
            AppEvent::StartCommitAnimation => {
                if self
                    .commit_anim_running
                    .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    let tx = self.app_event_tx.clone();
                    let running = self.commit_anim_running.clone();
                    thread::spawn(move || {
                        while running.load(Ordering::Relaxed) {
                            thread::sleep(COMMIT_ANIMATION_TICK);
                            tx.send(AppEvent::CommitTick);
                        }
                    });
                }
            }
            AppEvent::StopCommitAnimation => {
                self.commit_anim_running.store(false, Ordering::Release);
            }
            AppEvent::CommitTick => {
                self.chat_widget.on_commit_tick();
            }
            AppEvent::Exit(mode) => {
                return Ok(self.handle_exit_mode(app_gateway, mode).await);
            }
            AppEvent::FatalExitRequest(message) => {
                return Ok(AppRunControl::Exit(ExitReason::Fatal(message)));
            }
            AppEvent::AgentOp(op) => {
                self.submit_active_thread_op_or_start(tui, app_gateway, op.into())
                    .await?;
            }
            AppEvent::SubmitThreadOp { thread_id, op } => {
                self.submit_thread_op(app_gateway, thread_id, op.into())
                    .await?;
            }
            AppEvent::ThreadHistoryEntryResponse { thread_id, event } => {
                self.enqueue_thread_history_entry_response(thread_id, event)
                    .await?;
            }
            AppEvent::DiffResult(text) => {
                // Clear the in-progress state in the bottom pane
                self.chat_widget.on_diff_complete();
                // Enter alternate screen using TUI helper and build pager lines
                let _ = tui.enter_alt_screen();
                let pager_lines: Vec<ratatui::text::Line<'static>> = if text.trim().is_empty() {
                    vec!["No changes detected.".italic().into()]
                } else {
                    text.lines().map(ansi_escape_line).collect()
                };
                self.overlay = Some(Overlay::new_static_with_lines(
                    pager_lines,
                    "D I F F".to_string(),
                ));
                tui.frame_requester().schedule_frame();
            }
            AppEvent::OpenAppLink {
                app_id,
                title,
                description,
                instructions,
                url,
                is_installed,
                is_enabled,
            } => {
                self.chat_widget
                    .open_app_link_view(crate::bottom_pane::AppLinkViewParams {
                        app_id,
                        title,
                        description,
                        instructions,
                        url,
                        is_installed,
                        is_enabled,
                        suggest_reason: None,
                        suggestion_type: None,
                        elicitation_target: None,
                    });
            }
            AppEvent::OpenUrlInBrowser { url } => {
                self.open_url_in_browser(url);
            }
            AppEvent::RefreshConnectors { force_refetch } => {
                self.chat_widget.refresh_connectors(force_refetch);
            }
            AppEvent::PluginInstallAuthAdvance { refresh_connectors } => {
                if refresh_connectors {
                    self.chat_widget.refresh_connectors(/*force_refetch*/ true);
                }
                self.chat_widget.advance_plugin_install_auth_flow();
            }
            AppEvent::PluginInstallAuthAbandon => {
                self.chat_widget.abandon_plugin_install_auth_flow();
            }
            AppEvent::FetchPluginsList { cwd } => {
                self.fetch_plugins_list(app_gateway, cwd);
            }
            AppEvent::OpenPluginDetailLoading {
                plugin_display_name,
            } => {
                self.chat_widget
                    .open_plugin_detail_loading_popup(&plugin_display_name);
            }
            AppEvent::OpenPluginInstallLoading {
                plugin_display_name,
            } => {
                self.chat_widget
                    .open_plugin_install_loading_popup(&plugin_display_name);
            }
            AppEvent::OpenPluginUninstallLoading {
                plugin_display_name,
            } => {
                self.chat_widget
                    .open_plugin_uninstall_loading_popup(&plugin_display_name);
            }
            AppEvent::PluginsLoaded { cwd, result } => {
                self.chat_widget.on_plugins_loaded(cwd, result);
            }
            AppEvent::FetchPluginDetail { cwd, params } => {
                self.fetch_plugin_detail(app_gateway, cwd, params);
            }
            AppEvent::PluginDetailLoaded { cwd, result } => {
                self.chat_widget.on_plugin_detail_loaded(cwd, result);
            }
            AppEvent::FetchPluginInstall {
                cwd,
                marketplace_path,
                plugin_name,
                plugin_display_name,
            } => {
                self.fetch_plugin_install(
                    app_gateway,
                    cwd,
                    marketplace_path,
                    plugin_name,
                    plugin_display_name,
                );
            }
            AppEvent::FetchPluginUninstall {
                cwd,
                plugin_id,
                plugin_display_name,
            } => {
                self.fetch_plugin_uninstall(app_gateway, cwd, plugin_id, plugin_display_name);
            }
            AppEvent::PluginInstallLoaded {
                cwd,
                marketplace_path,
                plugin_name,
                plugin_display_name,
                result,
            } => {
                let install_succeeded = result.is_ok();
                if install_succeeded {
                    if let Err(err) = self.refresh_in_memory_config_from_disk().await {
                        tracing::warn!(error = %err, "failed to refresh config after plugin install");
                    }
                    self.chat_widget.refresh_plugin_mentions();
                    self.chat_widget.submit_op(AppCommand::reload_user_config());
                }
                let should_refresh_plugin_detail = self.chat_widget.on_plugin_install_loaded(
                    cwd.clone(),
                    marketplace_path.clone(),
                    plugin_name.clone(),
                    plugin_display_name,
                    result,
                );
                if install_succeeded && self.chat_widget.config_ref().cwd.as_path() == cwd.as_path()
                {
                    self.fetch_plugins_list(app_gateway, cwd.clone());
                    if should_refresh_plugin_detail {
                        self.fetch_plugin_detail(
                            app_gateway,
                            cwd,
                            PluginReadParams {
                                marketplace_path,
                                plugin_name,
                            },
                        );
                    }
                }
            }
            AppEvent::FetchMcpInventory => {
                self.fetch_mcp_inventory(app_gateway);
            }
            AppEvent::McpInventoryLoaded { result } => {
                self.handle_mcp_inventory_result(result);
            }
            AppEvent::StartFileSearch(query) => {
                self.file_search.on_user_query(query);
            }
            AppEvent::FileSearchResult { query, matches } => {
                self.chat_widget.apply_file_search_result(query, matches);
            }
            AppEvent::RefreshRateLimits { request_id } => {
                self.refresh_rate_limits(app_gateway, request_id);
            }
            AppEvent::RateLimitsLoaded { request_id, result } => match result {
                Ok(snapshots) => {
                    for snapshot in snapshots {
                        self.chat_widget.on_rate_limit_snapshot(Some(snapshot));
                    }
                    self.chat_widget
                        .finish_status_rate_limit_refresh(request_id);
                }
                Err(err) => {
                    tracing::warn!("account/rateLimits/read failed during TUI refresh: {err}");
                    self.chat_widget
                        .finish_status_rate_limit_refresh(request_id);
                }
            },
            AppEvent::WorkspaceThreadsLoaded { request_id, result } => {
                self.handle_workspace_threads_loaded(request_id, result);
                self.observe_existing_workspace_threads_if_needed(app_gateway)
                    .await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::WorkspaceSessionPickerPageLoaded { request, result } => {
                if self.workspace.apply_session_picker_page(&request, result) {
                    tui.frame_requester().schedule_frame();
                }
            }
            AppEvent::FetchTokenUsageSummary { limit } => {
                self.fetch_token_usage_summary(app_gateway, limit);
            }
            AppEvent::TokenUsageSummaryLoaded { limit, result } => {
                let cell = match result {
                    Ok(response) => {
                        crate::token_usage_summary::token_usage_summary_cell(limit, response)
                    }
                    Err(err) => history_cell::new_error_event(format!(
                        "Failed to summarize token usage: {err}"
                    )),
                };
                self.app_event_tx
                    .send(AppEvent::InsertHistoryCell(Box::new(cell)));
                tui.frame_requester().schedule_frame();
            }
            AppEvent::ConnectorsLoaded { result, is_final } => {
                self.chat_widget.on_connectors_loaded(result, is_final);
            }
            AppEvent::UpdateReasoningEffort(effort) => {
                self.on_update_reasoning_effort(effort);
            }
            AppEvent::UpdateModelSelection { model, provider_id } => {
                let provider = self.config.model_providers.get(&provider_id).cloned();
                if let Some(provider) = provider.as_ref() {
                    self.config.model_provider_id = provider_id.clone();
                    self.config.model_provider = provider.clone();
                }
                self.chat_widget
                    .set_model_selection(&model, &provider_id, provider.as_ref());
            }
            AppEvent::UpdateCollaborationMode(mask) => {
                self.chat_widget.set_collaboration_mask(mask);
            }
            AppEvent::UpdatePersonality(personality) => {
                self.on_update_personality(personality);
            }
            AppEvent::OpenRealtimeAudioDeviceSelection { kind } => {
                self.chat_widget.open_realtime_audio_device_selection(kind);
            }
            AppEvent::OpenReasoningPopup {
                model,
                provider_id,
                provider,
            } => {
                self.chat_widget
                    .open_reasoning_popup(model, provider_id, provider);
            }
            AppEvent::OpenPlanReasoningScopePrompt {
                model,
                provider_id,
                provider,
                effort,
            } => {
                self.chat_widget.open_plan_reasoning_scope_prompt(
                    model,
                    provider_id,
                    provider,
                    effort,
                );
            }
            AppEvent::OpenAllModelsPopup { models } => {
                self.chat_widget.open_all_models_popup(models);
            }
            AppEvent::OpenFullAccessConfirmation {
                preset,
                return_to_permissions,
            } => {
                self.chat_widget
                    .open_full_access_confirmation(preset, return_to_permissions);
            }
            AppEvent::OpenWorldWritableWarningConfirmation {
                preset,
                sample_paths,
                extra_count,
                failed_scan,
            } => {
                self.chat_widget.open_world_writable_warning_confirmation(
                    preset,
                    sample_paths,
                    extra_count,
                    failed_scan,
                );
            }
            AppEvent::OpenFeedbackNote {
                category,
                include_logs,
            } => {
                self.chat_widget.open_feedback_note(category, include_logs);
            }
            AppEvent::OpenFeedbackConsent { category } => {
                self.chat_widget.open_feedback_consent(category);
            }
            AppEvent::SubmitFeedback {
                category,
                reason,
                include_logs,
            } => {
                self.submit_feedback(app_gateway, category, reason, include_logs);
            }
            AppEvent::FeedbackSubmitted {
                origin_thread_id,
                category,
                include_logs,
                result,
            } => {
                self.handle_feedback_submitted(origin_thread_id, category, include_logs, result)
                    .await;
            }
            AppEvent::LaunchExternalEditor => {
                if self.chat_widget.external_editor_state() == ExternalEditorState::Active {
                    self.launch_external_editor(tui).await;
                }
            }
            AppEvent::OpenWindowsSandboxEnablePrompt { preset } => {
                self.chat_widget.open_windows_sandbox_enable_prompt(preset);
            }
            AppEvent::OpenWindowsSandboxRecoveryPrompt { preset } => {
                self.session_telemetry.counter(
                    "praxis.windows_sandbox.recovery_prompt_shown",
                    /*inc*/ 1,
                    &[],
                );
                self.chat_widget.clear_windows_sandbox_setup_status();
                if let Some(started_at) = self.windows_sandbox.setup_started_at.take() {
                    self.session_telemetry.record_duration(
                        "praxis.windows_sandbox.elevated_setup_duration_ms",
                        started_at.elapsed(),
                        &[("result", "failure")],
                    );
                }
                self.chat_widget.open_windows_sandbox_recovery_prompt(preset);
            }
            AppEvent::BeginWindowsSandboxElevatedSetup { preset } => {
                #[cfg(target_os = "windows")]
                {
                    let policy = preset.sandbox.clone();
                    let policy_cwd = self.config.cwd.clone();
                    let command_cwd = policy_cwd.clone();
                    let env_map: std::collections::HashMap<String, String> =
                        std::env::vars().collect();
                    let praxis_home = self.config.praxis_home.clone();
                    let tx = self.app_event_tx.clone();

                    // If the elevated setup already ran on this machine, don't prompt for
                    // elevation again - just flip the config to use the elevated path.
                    if praxis_core::windows_sandbox::sandbox_setup_is_complete(
                        praxis_home.as_path(),
                    ) {
                        tx.send(AppEvent::EnableWindowsSandboxForAgentMode {
                            preset,
                            mode: WindowsSandboxEnableMode::Elevated,
                        });
                        return Ok(AppRunControl::Continue);
                    }

                    self.chat_widget.show_windows_sandbox_setup_status();
                    self.windows_sandbox.setup_started_at = Some(Instant::now());
                    let session_telemetry = self.session_telemetry.clone();
                    tokio::task::spawn_blocking(move || {
                        let result = praxis_core::windows_sandbox::run_elevated_setup(
                            &policy,
                            policy_cwd.as_path(),
                            command_cwd.as_path(),
                            &env_map,
                            praxis_home.as_path(),
                        );
                        let event = match result {
                            Ok(()) => {
                                session_telemetry.counter(
                                    "praxis.windows_sandbox.elevated_setup_success",
                                    /*inc*/ 1,
                                    &[],
                                );
                                AppEvent::EnableWindowsSandboxForAgentMode {
                                    preset: preset.clone(),
                                    mode: WindowsSandboxEnableMode::Elevated,
                                }
                            }
                            Err(err) => {
                                let mut code_tag: Option<String> = None;
                                let mut message_tag: Option<String> = None;
                                if let Some((code, message)) =
                                    praxis_core::windows_sandbox::elevated_setup_failure_details(
                                        &err,
                                    )
                                {
                                    code_tag = Some(code);
                                    message_tag = Some(message);
                                }
                                let mut tags: Vec<(&str, &str)> = Vec::new();
                                if let Some(code) = code_tag.as_deref() {
                                    tags.push(("code", code));
                                }
                                if let Some(message) = message_tag.as_deref() {
                                    tags.push(("message", message));
                                }
                                session_telemetry.counter(
                                    praxis_core::windows_sandbox::elevated_setup_failure_metric_name(
                                        &err,
                                    ),
                                    /*inc*/ 1,
                                    &tags,
                                );
                                tracing::error!(
                                    error = %err,
                                    "failed to run elevated Windows sandbox setup"
                                );
                                AppEvent::OpenWindowsSandboxRecoveryPrompt { preset }
                            }
                        };
                        tx.send(event);
                    });
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = preset;
                }
            }
            AppEvent::BeginWindowsSandboxNonAdminSetup { preset } => {
                #[cfg(target_os = "windows")]
                {
                    let policy = preset.sandbox.clone();
                    let policy_cwd = self.config.cwd.clone();
                    let command_cwd = policy_cwd.clone();
                    let env_map: std::collections::HashMap<String, String> =
                        std::env::vars().collect();
                    let praxis_home = self.config.praxis_home.clone();
                    let tx = self.app_event_tx.clone();
                    let session_telemetry = self.session_telemetry.clone();

                    self.chat_widget.show_windows_sandbox_setup_status();
                    tokio::task::spawn_blocking(move || {
                        if let Err(err) = praxis_core::windows_sandbox::run_non_admin_setup_preflight(
                            &policy,
                            policy_cwd.as_path(),
                            command_cwd.as_path(),
                            &env_map,
                            praxis_home.as_path(),
                        ) {
                            session_telemetry.counter(
                                "praxis.windows_sandbox.non_admin_setup_preflight_failed",
                                /*inc*/ 1,
                                &[],
                            );
                            tracing::warn!(
                                error = %err,
                                "failed to preflight non-admin Windows sandbox setup"
                            );
                        }
                        tx.send(AppEvent::EnableWindowsSandboxForAgentMode {
                            preset,
                            mode: WindowsSandboxEnableMode::NonAdmin,
                        });
                    });
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = preset;
                }
            }
            AppEvent::BeginWindowsSandboxGrantReadRoot { path } => {
                #[cfg(target_os = "windows")]
                {
                    self.chat_widget
                        .add_to_history(history_cell::new_info_event(
                            format!("Granting sandbox read access to {path} ..."),
                            /*hint*/ None,
                        ));

                    let policy = self.config.permissions.sandbox_policy.get().clone();
                    let policy_cwd = self.config.cwd.clone();
                    let command_cwd = self.config.cwd.clone();
                    let env_map: std::collections::HashMap<String, String> =
                        std::env::vars().collect();
                    let praxis_home = self.config.praxis_home.clone();
                    let tx = self.app_event_tx.clone();

                    tokio::task::spawn_blocking(move || {
                        let requested_path = PathBuf::from(path);
                        let event = match praxis_core::windows_sandbox_read_grants::grant_read_root_non_elevated(
                            &policy,
                            policy_cwd.as_path(),
                            command_cwd.as_path(),
                            &env_map,
                            praxis_home.as_path(),
                            requested_path.as_path(),
                        ) {
                            Ok(canonical_path) => AppEvent::WindowsSandboxGrantReadRootCompleted {
                                path: canonical_path,
                                error: None,
                            },
                            Err(err) => AppEvent::WindowsSandboxGrantReadRootCompleted {
                                path: requested_path,
                                error: Some(err.to_string()),
                            },
                        };
                        tx.send(event);
                    });
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = path;
                }
            }
            AppEvent::WindowsSandboxGrantReadRootCompleted { path, error } => match error {
                Some(err) => {
                    self.chat_widget
                        .add_to_history(history_cell::new_error_event(format!("Error: {err}")));
                }
                None => {
                    self.chat_widget
                        .add_to_history(history_cell::new_info_event(
                            format!("Sandbox read access granted for {}", path.display()),
                            /*hint*/ None,
                        ));
                }
            },
            AppEvent::EnableWindowsSandboxForAgentMode { preset, mode } => {
                #[cfg(target_os = "windows")]
                {
                    self.chat_widget.clear_windows_sandbox_setup_status();
                    if let Some(started_at) = self.windows_sandbox.setup_started_at.take() {
                        self.session_telemetry.record_duration(
                            "praxis.windows_sandbox.elevated_setup_duration_ms",
                            started_at.elapsed(),
                            &[("result", "success")],
                        );
                    }
                    let profile = self.active_profile.as_deref();
                    let elevated_enabled = matches!(mode, WindowsSandboxEnableMode::Elevated);
                    let builder = ConfigEditsBuilder::new(&self.config.praxis_home)
                        .with_profile(profile)
                        .set_windows_sandbox_mode(if elevated_enabled {
                            "elevated"
                        } else {
                            "unelevated"
                        })
                        .clear_previous_windows_sandbox_keys();
                    match builder.apply().await {
                        Ok(()) => {
                            if elevated_enabled {
                                self.config.set_windows_sandbox_enabled(/*value*/ false);
                                self.config
                                    .set_windows_elevated_sandbox_enabled(/*value*/ true);
                            } else {
                                self.config.set_windows_sandbox_enabled(/*value*/ true);
                                self.config
                                    .set_windows_elevated_sandbox_enabled(/*value*/ false);
                            }
                            self.chat_widget.set_windows_sandbox_mode(
                                self.config.permissions.windows_sandbox_mode,
                            );
                            let windows_sandbox_level =
                                WindowsSandboxLevel::from_config(&self.config);
                            if let Some((sample_paths, extra_count, failed_scan)) =
                                self.chat_widget.world_writable_warning_details()
                            {
                                self.app_event_tx.send(AppEvent::AgentOp(
                                    AppCommand::override_turn_context(
                                        /*cwd*/ None,
                                        /*approval_policy*/ None,
                                        /*approvals_reviewer*/ None,
                                        /*sandbox_policy*/ None,
                                        #[cfg(target_os = "windows")]
                                        Some(windows_sandbox_level),
                                        /*model_provider*/ None,
                                        /*model*/ None,
                                        /*effort*/ None,
                                        /*summary*/ None,
                                        /*service_tier*/ None,
                                        /*collaboration_mode*/ None,
                                        /*personality*/ None,
                                    )
                                    .into(),
                                ));
                                self.app_event_tx.send(
                                    AppEvent::OpenWorldWritableWarningConfirmation {
                                        preset: Some(preset.clone()),
                                        sample_paths,
                                        extra_count,
                                        failed_scan,
                                    },
                                );
                            } else {
                                self.app_event_tx.send(AppEvent::AgentOp(
                                    AppCommand::override_turn_context(
                                        /*cwd*/ None,
                                        Some(preset.approval),
                                        Some(self.config.approvals_reviewer),
                                        Some(preset.sandbox.clone()),
                                        #[cfg(target_os = "windows")]
                                        Some(windows_sandbox_level),
                                        /*model_provider*/ None,
                                        /*model*/ None,
                                        /*effort*/ None,
                                        /*summary*/ None,
                                        /*service_tier*/ None,
                                        /*collaboration_mode*/ None,
                                        /*personality*/ None,
                                    )
                                    .into(),
                                ));
                                self.app_event_tx
                                    .send(AppEvent::UpdateAskForApprovalPolicy(preset.approval));
                                self.app_event_tx
                                    .send(AppEvent::UpdateSandboxPolicy(preset.sandbox.clone()));
                                let _ = mode;
                                self.chat_widget.add_plain_history_lines(vec![
                                    Line::from(vec!["• ".dim(), "Sandbox ready".into()]),
                                    Line::from(vec![
                                        "  ".into(),
                                        "Praxis can now safely edit files and execute commands in your computer"
                                            .dark_gray(),
                                    ]),
                                ]);
                            }
                        }
                        Err(err) => {
                            tracing::error!(
                                error = %err,
                                "failed to enable Windows sandbox feature"
                            );
                            self.chat_widget.add_error_message(format!(
                                "Failed to enable the Windows sandbox feature: {err}"
                            ));
                        }
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (preset, mode);
                }
            }
            AppEvent::OpenApprovalsPopup => {
                self.chat_widget.open_approvals_popup();
            }
            AppEvent::OpenAgentPicker => {
                if !self.workspace.enabled {
                    if let Err(err) = tui.set_mouse_capture_enabled(true) {
                        tracing::warn!(error = %err, "failed to enable mouse capture for Workspace agent picker");
                    }
                    self.workspace.enabled = true;
                    self.refresh_workspace_threads(app_gateway, true);
                }
                self.open_agent_picker(app_gateway).await;
                tui.frame_requester().schedule_frame();
            }
            AppEvent::SelectAgentThread(thread_id) => {
                self.select_agent_thread(tui, app_gateway, thread_id)
                    .await?;
            }
            AppEvent::OpenSkillsList => {
                self.chat_widget.open_skills_list();
            }
            AppEvent::OpenManageSkillsPopup => {
                self.chat_widget.open_manage_skills_popup();
            }
            AppEvent::SetSkillEnabled { path, enabled } => {
                let edits = [ConfigEdit::SetSkillConfig {
                    path: path.clone(),
                    enabled,
                }];
                match ConfigEditsBuilder::new(&self.config.praxis_home)
                    .with_edits(edits)
                    .apply()
                    .await
                {
                    Ok(()) => {
                        self.chat_widget.update_skill_enabled(path.clone(), enabled);
                        if let Err(err) = self.refresh_in_memory_config_from_disk().await {
                            tracing::warn!(
                                error = %err,
                                "failed to refresh config after skill toggle"
                            );
                        }
                    }
                    Err(err) => {
                        let path_display = path.display();
                        self.chat_widget.add_error_message(format!(
                            "Failed to update skill config for {path_display}: {err}"
                        ));
                    }
                }
            }
            AppEvent::SetAppEnabled { id, enabled } => {
                let edits = if enabled {
                    vec![
                        ConfigEdit::ClearPath {
                            segments: vec!["apps".to_string(), id.clone(), "enabled".to_string()],
                        },
                        ConfigEdit::ClearPath {
                            segments: vec![
                                "apps".to_string(),
                                id.clone(),
                                "disabled_reason".to_string(),
                            ],
                        },
                    ]
                } else {
                    vec![
                        ConfigEdit::SetPath {
                            segments: vec!["apps".to_string(), id.clone(), "enabled".to_string()],
                            value: false.into(),
                        },
                        ConfigEdit::SetPath {
                            segments: vec![
                                "apps".to_string(),
                                id.clone(),
                                "disabled_reason".to_string(),
                            ],
                            value: "user".into(),
                        },
                    ]
                };
                match ConfigEditsBuilder::new(&self.config.praxis_home)
                    .with_edits(edits)
                    .apply()
                    .await
                {
                    Ok(()) => {
                        self.chat_widget.update_connector_enabled(&id, enabled);
                        if let Err(err) = self.refresh_in_memory_config_from_disk().await {
                            tracing::warn!(error = %err, "failed to refresh config after app toggle");
                        }
                        self.chat_widget.submit_op(AppCommand::reload_user_config());
                    }
                    Err(err) => {
                        self.chat_widget.add_error_message(format!(
                            "Failed to update app config for {id}: {err}"
                        ));
                    }
                }
            }
            AppEvent::OpenPermissionsPopup => {
                self.chat_widget.open_permissions_popup();
            }
            AppEvent::OpenReviewBranchPicker(cwd) => {
                self.chat_widget.show_review_branch_picker(&cwd).await;
            }
            AppEvent::OpenReviewCommitPicker(cwd) => {
                self.chat_widget.show_review_commit_picker(&cwd).await;
            }
            AppEvent::OpenReviewCustomPrompt => {
                self.chat_widget.show_review_custom_prompt();
            }
            AppEvent::SubmitUserMessageWithMode {
                text,
                collaboration_mode,
            } => {
                self.chat_widget
                    .submit_user_message_with_mode(text, collaboration_mode);
            }
            AppEvent::PersistSelfworkPlanPath {
                thread_id,
                plan_path,
            } => match app_gateway
                .thread_set_selfwork_plan_path(thread_id, plan_path.clone())
                .await
            {
                Ok(()) => {
                    self.update_thread_session_selfwork_plan_path(thread_id, plan_path)
                        .await;
                }
                Err(err) => {
                    self.chat_widget
                        .add_error_message(format!("Failed to persist selfwork plan state: {err}"));
                }
            },
            AppEvent::ActivateSelfworkPlan { plan_path } => {
                self.chat_widget.activate_selfwork(plan_path);
            }
            AppEvent::OpenSelfworkPlanPrompt => {
                self.chat_widget.show_selfwork_plan_prompt();
            }
            AppEvent::StartSelfworkFromInput { raw_path } => {
                self.chat_widget.start_selfwork_from_input(raw_path);
            }
            AppEvent::ManageSkillsClosed => {
                self.chat_widget.handle_manage_skills_closed();
            }
            AppEvent::FullScreenApprovalRequest(request) => match request {
                ApprovalRequest::ApplyPatch { cwd, changes, .. } => {
                    let _ = tui.enter_alt_screen();
                    let diff_summary = DiffSummary::new(changes, cwd);
                    self.overlay = Some(Overlay::new_static_with_renderables(
                        vec![diff_summary.into()],
                        "P A T C H".to_string(),
                    ));
                }
                ApprovalRequest::Exec { command, .. } => {
                    let _ = tui.enter_alt_screen();
                    let full_cmd = strip_bash_lc_and_escape(&command);
                    let full_cmd_lines = highlight_bash_to_lines(&full_cmd);
                    self.overlay = Some(Overlay::new_static_with_lines(
                        full_cmd_lines,
                        "E X E C".to_string(),
                    ));
                }
                ApprovalRequest::Permissions {
                    permissions,
                    reason,
                    ..
                } => {
                    let _ = tui.enter_alt_screen();
                    let mut lines = Vec::new();
                    if let Some(reason) = reason {
                        lines.push(Line::from(vec!["Reason: ".into(), reason.italic()]));
                        lines.push(Line::from(""));
                    }
                    if let Some(rule_line) =
                        crate::bottom_pane::format_requested_permissions_rule(&permissions)
                    {
                        lines.push(Line::from(vec![
                            "Permission rule: ".into(),
                            rule_line.cyan(),
                        ]));
                    }
                    self.overlay = Some(Overlay::new_static_with_renderables(
                        vec![Box::new(Paragraph::new(lines).wrap(Wrap { trim: false }))],
                        "P E R M I S S I O N S".to_string(),
                    ));
                }
                ApprovalRequest::McpElicitation {
                    server_name,
                    message,
                    ..
                } => {
                    let _ = tui.enter_alt_screen();
                    let paragraph = Paragraph::new(vec![
                        Line::from(vec!["Server: ".into(), server_name.bold()]),
                        Line::from(""),
                        Line::from(message),
                    ])
                    .wrap(Wrap { trim: false });
                    self.overlay = Some(Overlay::new_static_with_renderables(
                        vec![Box::new(paragraph)],
                        "E L I C I T A T I O N".to_string(),
                    ));
                }
            },
            #[cfg(not(target_os = "linux"))]
            AppEvent::UpdateRecordingMeter { id, text } => {
                // Update in place to preserve the element id for subsequent frames.
                let updated = self.chat_widget.update_recording_meter_in_place(&id, &text);
                if updated
                    || self
                        .chat_widget
                        .stop_realtime_conversation_for_deleted_meter(&id)
                {
                    tui.frame_requester().schedule_frame();
                }
            }
            AppEvent::StatusLineSetup { items } => {
                self.handle_status_line_setup(items).await;
            }
            AppEvent::StatusLineBranchUpdated { cwd, branch } => {
                self.handle_status_line_branch_updated(cwd, branch);
            }
            AppEvent::StatusLineSetupCancelled => {
                self.handle_status_line_setup_cancelled();
            }
            AppEvent::TerminalTitleSetup { items } => {
                self.handle_terminal_title_setup(items).await;
            }
            AppEvent::TerminalTitleSetupPreview { items } => {
                self.handle_terminal_title_setup_preview(items);
            }
            AppEvent::TerminalTitleSetupCancelled => {
                self.handle_terminal_title_setup_cancelled();
            }
            AppEvent::SyntaxThemeSelected { name } => {
                self.handle_syntax_theme_selected(name).await;
            }
            AppEvent::SurfaceThemePreview { name } => {
                self.handle_surface_theme_preview(tui, name);
            }
            AppEvent::SurfaceThemeSelected {
                name,
                previous_name,
            } => {
                self.handle_surface_theme_selected(tui, name, previous_name)
                    .await;
            }
            event @ (AppEvent::OpenProviderLoginPrompt { .. }
            | AppEvent::ShowAnthropicLoginStatement
            | AppEvent::ApplyProviderSetup { .. }
            | AppEvent::ApplyModelSelection { .. }
            | AppEvent::PluginUninstallLoaded { .. }
            | AppEvent::PersistPersonalitySelection { .. }
            | AppEvent::PersistServiceTierSelection { .. }
            | AppEvent::PersistRealtimeAudioDeviceSelection { .. }
            | AppEvent::RestartRealtimeAudioDevice { .. }
            | AppEvent::UpdateAskForApprovalPolicy(_)
            | AppEvent::UpdateSandboxPolicy(_)
            | AppEvent::UpdateApprovalsReviewer(_)
            | AppEvent::UpdateFeatureFlags { .. }
            | AppEvent::SkipNextWorldWritableScan
            | AppEvent::UpdateFullAccessWarningAcknowledged(_)
            | AppEvent::UpdateWorldWritableWarningAcknowledged(_)
            | AppEvent::UpdateRateLimitSwitchPromptHidden(_)
            | AppEvent::UpdatePlanModeReasoningEffort(_)
            | AppEvent::PersistFullAccessWarningAcknowledged
            | AppEvent::PersistWorldWritableWarningAcknowledged
            | AppEvent::PersistRateLimitSwitchPromptHidden
            | AppEvent::PersistPlanModeReasoningEffort(_)
            | AppEvent::PersistModelMigrationPromptAcknowledged { .. }) => {
                return self
                    .handle_provider_policy_event(app_gateway, event)
                    .await;
            }
        }
        Ok(AppRunControl::Continue)
    }
}

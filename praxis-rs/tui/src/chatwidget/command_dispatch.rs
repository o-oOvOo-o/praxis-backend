use super::*;
use praxis_app_gateway_protocol::ThreadGoalStatus as AppGatewayThreadGoalStatus;

impl ChatWidget {
    pub(super) fn dispatch_command(&mut self, cmd: SlashCommand) {
        if !cmd.available_during_task() && self.bottom_pane.is_task_running() {
            let message = format!(
                "'/{}' is disabled while a task is in progress.",
                cmd.command()
            );
            self.add_to_history(history_cell::new_error_event(message));
            self.bottom_pane.drain_pending_submission_state();
            self.request_redraw();
            return;
        }
        match cmd {
            SlashCommand::Feedback => {
                if !self.config.feedback_enabled {
                    let params = crate::bottom_pane::feedback_disabled_params();
                    self.bottom_pane.show_selection_view(params);
                    self.request_redraw();
                    return;
                }
                // Step 1: pick a category (UI built in feedback_view)
                let params =
                    crate::bottom_pane::feedback_selection_params(self.app_event_tx.clone());
                self.bottom_pane.show_selection_view(params);
                self.request_redraw();
            }
            SlashCommand::New => {
                self.app_event_tx.send(AppEvent::NewSession);
            }
            SlashCommand::Clear => {
                self.app_event_tx.send(AppEvent::ClearUi);
            }
            SlashCommand::Resume => {
                self.app_event_tx.send(AppEvent::OpenResumePicker);
            }
            SlashCommand::Fork => {
                self.app_event_tx.send(AppEvent::ForkCurrentSession);
            }
            SlashCommand::Codex | SlashCommand::Cursor => {
                self.dispatch_external_thread_command(ExternalThreadCommandIntent {
                    source: match cmd {
                        SlashCommand::Codex => ExternalThreadCommandSource::Codex,
                        SlashCommand::Cursor => ExternalThreadCommandSource::Cursor,
                        _ => unreachable!(),
                    },
                    action: ExternalThreadCommandAction::Resume,
                });
            }
            SlashCommand::Init => {
                let init_target = match self.config.cwd.join(DEFAULT_PROJECT_DOC_FILENAME) {
                    Ok(path) => path,
                    Err(err) => {
                        self.add_error_message(format!(
                            "Failed to prepare {DEFAULT_PROJECT_DOC_FILENAME}: {err}",
                        ));
                        return;
                    }
                };
                if init_target.exists() {
                    let message = format!(
                        "{DEFAULT_PROJECT_DOC_FILENAME} already exists here. Skipping /init to avoid overwriting it."
                    );
                    self.add_info_message(message, /*hint*/ None);
                    return;
                }
                const INIT_PROMPT: &str = include_str!("../../prompt_for_init_command.md");
                self.submit_user_message(INIT_PROMPT.to_string().into());
            }
            SlashCommand::Compact => {
                self.clear_token_usage();
                if !self.bottom_pane.is_task_running() {
                    self.bottom_pane.set_task_running(/*running*/ true);
                }
                self.app_event_tx.compact();
            }
            SlashCommand::Review => {
                self.open_review_popup();
            }
            SlashCommand::Rename => {
                self.session_telemetry
                    .counter("praxis.thread.rename", /*inc*/ 1, &[]);
                self.show_rename_prompt();
            }
            SlashCommand::Namegen => {
                self.session_telemetry
                    .counter("praxis.thread.namegen", /*inc*/ 1, &[]);
                let Some(thread_id) = self.thread_id else {
                    self.add_error_message(
                        "Thread is not ready yet; cannot regenerate its name.".to_string(),
                    );
                    self.bottom_pane.drain_pending_submission_state();
                    return;
                };
                self.app_event_tx
                    .send(AppEvent::RegenerateThreadName { thread_id });
                self.add_info_message(
                    "Regenerating thread name.".to_string(),
                    Some(
                        "Praxis will rename this thread from the current conversation context."
                            .to_string(),
                    ),
                );
                self.bottom_pane.drain_pending_submission_state();
            }
            SlashCommand::Model => {
                self.open_model_popup();
            }
            SlashCommand::Login => {
                self.open_login_popup();
            }
            SlashCommand::Fast => {
                let next_tier = if matches!(self.config.service_tier, Some(ServiceTier::Fast)) {
                    None
                } else {
                    Some(ServiceTier::Fast)
                };
                self.set_service_tier_selection(next_tier);
            }
            SlashCommand::Realtime => {
                if !self.realtime_conversation_enabled() {
                    return;
                }
                if self.realtime_conversation.is_live() {
                    self.stop_realtime_conversation_from_ui();
                } else {
                    self.start_realtime_conversation();
                }
            }
            SlashCommand::Settings => {
                if !self.realtime_audio_device_selection_enabled() {
                    return;
                }
                self.open_realtime_audio_popup();
            }
            SlashCommand::Personality => {
                self.open_personality_popup();
            }
            SlashCommand::Plan => {
                if !self.collaboration_modes_enabled() {
                    self.add_info_message(
                        "Collaboration modes are disabled.".to_string(),
                        Some("Enable collaboration modes to use /plan.".to_string()),
                    );
                    return;
                }
                if let Some(mask) = collaboration_modes::plan_mask(self.model_catalog.as_ref()) {
                    self.set_collaboration_mask(mask);
                } else {
                    self.add_info_message(
                        "Plan mode unavailable right now.".to_string(),
                        /*hint*/ None,
                    );
                }
            }
            SlashCommand::Goal => {
                self.dispatch_goal_command(None);
            }
            SlashCommand::ReleaseThread => {
                if self.thread_control_state.is_some() {
                    self.app_event_tx
                        .send(AppEvent::ReleaseCurrentThreadControl);
                    self.add_info_message(
                        "Releasing the current thread lock.".to_string(),
                        Some("External or rank controllers can acquire it again on their next action.".to_string()),
                    );
                } else {
                    self.add_info_message(
                        "This thread is not locked.".to_string(),
                        Some("Use /release-thread only on threads controlled by an external or rank controller.".to_string()),
                    );
                }
                self.bottom_pane.drain_pending_submission_state();
            }
            SlashCommand::Collab => {
                if !self.collaboration_modes_enabled() {
                    self.add_info_message(
                        "Collaboration modes are disabled.".to_string(),
                        Some("Enable collaboration modes to use /collab.".to_string()),
                    );
                    return;
                }
                self.open_collaboration_modes_popup();
            }
            SlashCommand::Agent | SlashCommand::MultiAgents => {
                self.app_event_tx.send(AppEvent::OpenAgentPicker);
            }
            SlashCommand::Approvals => {
                self.open_permissions_popup();
            }
            SlashCommand::Permissions => {
                self.open_permissions_popup();
            }
            SlashCommand::ElevateSandbox => {
                #[cfg(target_os = "windows")]
                {
                    let windows_sandbox_level = WindowsSandboxLevel::from_config(&self.config);
                    let windows_degraded_sandbox_enabled =
                        matches!(windows_sandbox_level, WindowsSandboxLevel::RestrictedToken);
                    if !windows_degraded_sandbox_enabled
                        || !praxis_core::windows_sandbox::ELEVATED_SANDBOX_NUX_ENABLED
                    {
                        // This command should not be visible/recognized outside degraded mode,
                        // but guard anyway in case something dispatches it directly.
                        return;
                    }

                    let Some(preset) = builtin_approval_presets()
                        .into_iter()
                        .find(|preset| preset.id == "auto")
                    else {
                        // Avoid panicking in interactive UI; treat this as a recoverable
                        // internal error.
                        self.add_error_message(
                            "Internal error: missing the 'auto' approval preset.".to_string(),
                        );
                        return;
                    };

                    if let Err(err) = self
                        .config
                        .permissions
                        .approval_policy
                        .can_set(&preset.approval)
                    {
                        self.add_error_message(err.to_string());
                        return;
                    }

                    self.session_telemetry.counter(
                        "praxis.windows_sandbox.setup_elevated_sandbox_command",
                        /*inc*/ 1,
                        &[],
                    );
                    self.app_event_tx
                        .send(AppEvent::BeginWindowsSandboxElevatedSetup { preset });
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = &self.session_telemetry;
                    // Not supported; on non-Windows this command should never be reachable.
                };
            }
            SlashCommand::SandboxReadRoot => {
                self.add_error_message(
                    "Usage: /sandbox-add-read-dir <absolute-directory-path>".to_string(),
                );
            }
            SlashCommand::Experimental => {
                self.open_experimental_popup();
            }
            SlashCommand::Quit | SlashCommand::Exit => {
                self.request_quit_without_confirmation();
            }
            SlashCommand::Logout => {
                if let Err(e) = praxis_login::logout(
                    &self.config.praxis_home,
                    self.config.cli_auth_credentials_store_mode,
                ) {
                    tracing::error!("failed to logout: {e}");
                }
                self.request_quit_without_confirmation();
            }
            // SlashCommand::Undo => {
            //     self.app_event_tx.send(AppEvent::AgentOp(Op::Undo));
            // }
            SlashCommand::Diff => {
                self.add_diff_in_progress();
                let tx = self.app_event_tx.clone();
                tokio::spawn(async move {
                    let text = match get_git_diff().await {
                        Ok((is_git_repo, diff_text)) => {
                            if is_git_repo {
                                diff_text
                            } else {
                                "`/diff` — _not inside a git repository_".to_string()
                            }
                        }
                        Err(e) => format!("Failed to compute diff: {e}"),
                    };
                    tx.send(AppEvent::DiffResult(text));
                });
            }
            SlashCommand::Copy => {
                let Some(text) = self.last_copyable_output.as_deref() else {
                    self.add_info_message(
                        "`/copy` is unavailable before the first Praxis output or right after a rollback."
                            .to_string(),
                        /*hint*/ None,
                    );
                    return;
                };

                let copy_result = clipboard_text::copy_text_to_clipboard(text);

                match copy_result {
                    Ok(()) => {
                        self.show_info_toast("Copied latest Praxis output to clipboard.");
                        let hint = self.agent_turn_running.then_some(
                            "Current turn is still running; copied the latest completed output (not the in-progress response)."
                                .to_string(),
                        );
                        self.add_info_message(
                            "Copied latest Praxis output to clipboard.".to_string(),
                            hint,
                        );
                    }
                    Err(err) => {
                        self.add_error_message(format!("Failed to copy to clipboard: {err}"))
                    }
                }
            }
            SlashCommand::Mention => {
                self.insert_str("@");
            }
            SlashCommand::Skills => {
                self.open_skills_menu();
            }
            SlashCommand::Status => {
                if self.should_prefetch_rate_limits() {
                    let request_id = self.next_status_refresh_request_id;
                    self.next_status_refresh_request_id =
                        self.next_status_refresh_request_id.wrapping_add(1);
                    self.add_status_output(/*refreshing_rate_limits*/ true, Some(request_id));
                    self.app_event_tx
                        .send(AppEvent::RefreshRateLimits { request_id });
                } else {
                    self.add_status_output(
                        /*refreshing_rate_limits*/ false, /*request_id*/ None,
                    );
                }
            }
            SlashCommand::Token => {
                self.app_event_tx.send(AppEvent::FetchTokenUsageSummary {
                    limit: crate::token_usage_summary::DEFAULT_TOKEN_USAGE_THREAD_LIMIT,
                });
                self.bottom_pane.drain_pending_submission_state();
            }
            SlashCommand::DebugConfig => {
                self.add_debug_config_output();
            }
            SlashCommand::Title => {
                self.open_terminal_title_setup();
            }
            SlashCommand::Statusline => {
                self.open_status_line_setup();
            }
            SlashCommand::Theme => {
                self.open_theme_picker();
            }
            SlashCommand::SurfaceTheme => {
                self.open_surface_theme_picker();
            }
            SlashCommand::Language => {
                self.handle_language_command("");
            }
            SlashCommand::Ps => {
                self.add_ps_output();
            }
            SlashCommand::Stop => {
                self.clean_background_terminals();
            }
            SlashCommand::MemoryDrop => {
                self.add_app_gateway_stub_message("Memory maintenance");
            }
            SlashCommand::MemoryUpdate => {
                self.add_app_gateway_stub_message("Memory maintenance");
            }
            SlashCommand::Selfwork => {
                self.handle_selfwork_default_invocation();
            }
            SlashCommand::Mcp => {
                self.add_mcp_output();
            }
            SlashCommand::Apps => {
                self.add_connectors_output();
            }
            SlashCommand::Plugins => {
                self.add_plugins_output();
            }
            SlashCommand::Rollout => {
                if let Some(path) = self.rollout_path() {
                    self.add_info_message(
                        format!("Current rollout path: {}", path.display()),
                        /*hint*/ None,
                    );
                } else {
                    self.add_info_message(
                        "Rollout path is not available yet.".to_string(),
                        /*hint*/ None,
                    );
                }
            }
            SlashCommand::TestApproval => {
                use std::collections::HashMap;

                use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
                use praxis_protocol::protocol::FileChange;

                self.on_apply_patch_approval_request(
                    "1".to_string(),
                    ApplyPatchApprovalRequestEvent {
                        call_id: "1".to_string(),
                        turn_id: "turn-1".to_string(),
                        changes: HashMap::from([
                            (
                                PathBuf::from("/tmp/test.txt"),
                                FileChange::Add {
                                    content: "test".to_string(),
                                },
                            ),
                            (
                                PathBuf::from("/tmp/test2.txt"),
                                FileChange::Update {
                                    unified_diff: "+test\n-test2".to_string(),
                                    move_path: None,
                                },
                            ),
                        ]),
                        reason: None,
                        grant_root: Some(PathBuf::from("/tmp")),
                    },
                );
            }
        }
    }

    pub(super) fn dispatch_external_thread_command(
        &mut self,
        intent: ExternalThreadCommandIntent,
    ) {
        let action = match intent.action {
            ExternalThreadCommandAction::Resume => SessionPickerAction::Resume,
            ExternalThreadCommandAction::Fork => SessionPickerAction::Fork,
        };
        let source = match intent.source {
            ExternalThreadCommandSource::Codex => SessionLookupSource::Codex,
            ExternalThreadCommandSource::Cursor => SessionLookupSource::Cursor,
        };
        self.app_event_tx
            .send(AppEvent::OpenThreadPicker { source, action });
    }

    pub(super) fn dispatch_command_with_args(
        &mut self,
        cmd: SlashCommand,
        args: String,
        _text_elements: Vec<TextElement>,
    ) {
        if !cmd.supports_inline_args() {
            self.dispatch_command(cmd);
            return;
        }
        if !cmd.available_during_task() && self.bottom_pane.is_task_running() {
            let message = format!(
                "'/{}' is disabled while a task is in progress.",
                cmd.command()
            );
            self.add_to_history(history_cell::new_error_event(message));
            self.request_redraw();
            return;
        }

        let trimmed = args.trim();
        match cmd {
            SlashCommand::Codex | SlashCommand::Cursor => {
                let command_name = cmd.command();
                let command = if trimmed.is_empty() {
                    format!("/{command_name}")
                } else {
                    format!("/{command_name} {trimmed}")
                };
                match parse_external_thread_command(command.as_str()) {
                    Some(intent) => self.dispatch_external_thread_command(intent),
                    None => self.add_error_message(format!(
                        "Usage: /{command_name} [resume|fork|threads|list]"
                    )),
                }
            }
            SlashCommand::Language => {
                self.handle_language_command(trimmed);
            }
            SlashCommand::Token => {
                match crate::token_usage_summary::parse_token_usage_limit(trimmed) {
                    Ok(limit) => {
                        let _ = self
                            .bottom_pane
                            .prepare_inline_args_submission(/*record_history*/ false);
                        self.app_event_tx
                            .send(AppEvent::FetchTokenUsageSummary { limit });
                        self.bottom_pane.drain_pending_submission_state();
                    }
                    Err(message) => self.add_error_message(message),
                }
            }
            SlashCommand::Login => {
                self.handle_login_command_args(trimmed);
            }
            SlashCommand::Fast => {
                if trimmed.is_empty() {
                    self.dispatch_command(cmd);
                    return;
                }
                match trimmed.to_ascii_lowercase().as_str() {
                    "on" => self.set_service_tier_selection(Some(ServiceTier::Fast)),
                    "off" => self.set_service_tier_selection(/*service_tier*/ None),
                    "status" => {
                        let status = if matches!(self.config.service_tier, Some(ServiceTier::Fast))
                        {
                            "on"
                        } else {
                            "off"
                        };
                        self.add_info_message(
                            format!("Fast mode is {status}."),
                            /*hint*/ None,
                        );
                    }
                    _ => {
                        self.add_error_message("Usage: /fast [on|off|status]".to_string());
                    }
                }
            }
            SlashCommand::Rename if !trimmed.is_empty() => {
                self.session_telemetry
                    .counter("praxis.thread.rename", /*inc*/ 1, &[]);
                let Some((prepared_args, _prepared_elements)) = self
                    .bottom_pane
                    .prepare_inline_args_submission(/*record_history*/ false)
                else {
                    return;
                };
                let Some(name) = praxis_core::util::normalize_thread_name(&prepared_args) else {
                    self.add_error_message("Thread name cannot be empty.".to_string());
                    return;
                };
                let cell = Self::rename_confirmation_cell(&name, self.thread_id);
                self.add_boxed_history(Box::new(cell));
                self.request_redraw();
                self.app_event_tx.set_thread_name(name);
                self.bottom_pane.drain_pending_submission_state();
            }
            SlashCommand::Plan if !trimmed.is_empty() => {
                self.dispatch_command(cmd);
                if self.active_mode_kind() != ModeKind::Plan {
                    return;
                }
                let Some((prepared_args, prepared_elements)) = self
                    .bottom_pane
                    .prepare_inline_args_submission(/*record_history*/ true)
                else {
                    return;
                };
                let local_images = self
                    .bottom_pane
                    .take_recent_submission_images_with_placeholders();
                let remote_image_urls = self.take_remote_image_urls();
                let user_message = UserMessage {
                    text: prepared_args,
                    local_images,
                    remote_image_urls,
                    text_elements: prepared_elements,
                    mention_bindings: self.bottom_pane.take_recent_submission_mention_bindings(),
                };
                if self.is_session_configured() {
                    self.reasoning_buffer.clear();
                    self.full_reasoning_buffer.clear();
                    self.reasoning_block_kind = None;
                    self.set_status_header(GENERIC_STATUS_HEADER.to_string());
                    self.submit_user_message(user_message);
                } else {
                    self.queue_user_message(user_message);
                }
            }
            SlashCommand::Goal => {
                let Some((prepared_args, _prepared_elements)) = self
                    .bottom_pane
                    .prepare_inline_args_submission(/*record_history*/ true)
                else {
                    return;
                };
                self.dispatch_goal_command(Some(prepared_args));
            }
            SlashCommand::Review if !trimmed.is_empty() => {
                let Some((prepared_args, _prepared_elements)) = self
                    .bottom_pane
                    .prepare_inline_args_submission(/*record_history*/ false)
                else {
                    return;
                };
                self.submit_op(AppCommand::review(ReviewRequest {
                    target: ReviewTarget::Custom {
                        instructions: prepared_args,
                    },
                    user_facing_hint: None,
                }));
                self.bottom_pane.drain_pending_submission_state();
            }
            SlashCommand::SandboxReadRoot if !trimmed.is_empty() => {
                let Some((prepared_args, _prepared_elements)) = self
                    .bottom_pane
                    .prepare_inline_args_submission(/*record_history*/ false)
                else {
                    return;
                };
                self.app_event_tx
                    .send(AppEvent::BeginWindowsSandboxGrantReadRoot {
                        path: prepared_args,
                    });
                self.bottom_pane.drain_pending_submission_state();
            }
            SlashCommand::Selfwork => {
                let Some((prepared_args, _prepared_elements)) = self
                    .bottom_pane
                    .prepare_inline_args_submission(/*record_history*/ false)
                else {
                    return;
                };
                let prepared_trimmed = prepared_args.trim();
                if prepared_trimmed.is_empty() {
                    self.handle_selfwork_default_invocation();
                    self.bottom_pane.drain_pending_submission_state();
                    return;
                }

                let mut parts = prepared_trimmed.splitn(2, char::is_whitespace);
                let command = parts.next().unwrap_or_default();
                let rest = parts.next().unwrap_or_default().trim();
                match command.to_ascii_lowercase().as_str() {
                    "status" => self.show_selfwork_status(),
                    "stop" => {
                        let message = if self.selfwork_turn_in_flight {
                            "Selfwork stopped. The in-flight selfwork turn will finish, but it will not auto-continue.".to_string()
                        } else {
                            "Selfwork stopped.".to_string()
                        };
                        self.clear_selfwork_state(/*persist*/ true, Some(message), None);
                    }
                    "start" if rest.is_empty() => self.open_selfwork_plan_picker_or_prompt(),
                    "start" => self.start_selfwork_from_input(rest.to_string()),
                    _ => match resolve_selfwork_plan_path(
                        prepared_trimmed,
                        self.current_cwd.as_deref(),
                        self.config.cwd.as_path(),
                    ) {
                        Ok(path) => self.activate_selfwork(path),
                        Err(err) => self.add_error_message(err),
                    },
                }
                self.bottom_pane.drain_pending_submission_state();
            }
            _ => self.dispatch_command(cmd),
        }
    }

    fn dispatch_goal_command(&mut self, args: Option<String>) {
        let trimmed = args.as_deref().unwrap_or_default().trim();
        let command = trimmed.to_ascii_lowercase();
        let Some(thread_id) = self.thread_id else {
            self.add_error_message("No active thread is available for /goal.".to_string());
            self.bottom_pane.drain_pending_submission_state();
            return;
        };

        match command.as_str() {
            "" | "status" => {
                self.app_event_tx
                    .send(AppEvent::OpenThreadGoalMenu { thread_id });
            }
            "pause" => {
                self.app_event_tx.send(AppEvent::SetThreadGoalStatus {
                    thread_id,
                    status: AppGatewayThreadGoalStatus::Paused,
                });
            }
            "resume" => {
                self.app_event_tx.send(AppEvent::SetThreadGoalStatus {
                    thread_id,
                    status: AppGatewayThreadGoalStatus::Active,
                });
            }
            "complete" => {
                self.app_event_tx.send(AppEvent::SetThreadGoalStatus {
                    thread_id,
                    status: AppGatewayThreadGoalStatus::Complete,
                });
            }
            "block" | "blocked" => {
                self.app_event_tx.send(AppEvent::SetThreadGoalStatus {
                    thread_id,
                    status: AppGatewayThreadGoalStatus::Blocked,
                });
            }
            "clear" => {
                self.app_event_tx
                    .send(AppEvent::ClearThreadGoal { thread_id });
            }
            "edit" => {
                self.app_event_tx.send(AppEvent::OpenThreadGoalEditor {
                    thread_id: Some(thread_id),
                });
            }
            _ => {
                self.app_event_tx.send(AppEvent::SetThreadGoalObjective {
                    thread_id,
                    objective: trimmed.to_string(),
                    mode: ThreadGoalSetMode::ReplaceExisting,
                });
            }
        }
        self.bottom_pane.drain_pending_submission_state();
    }

    pub(super) fn dispatch_slash_command_from_user_message(
        &mut self,
        user_message: &UserMessage,
    ) -> bool {
        if !user_message.text.starts_with('/') {
            return false;
        }
        let Some((name, rest, _rest_offset)) =
            crate::bottom_pane::parse_slash_name(&user_message.text)
        else {
            return false;
        };
        if let Some(intent) = parse_external_thread_command(user_message.text.as_str()) {
            self.dispatch_external_thread_command(intent);
            return true;
        }
        let Ok(cmd) = SlashCommand::from_str(name) else {
            return false;
        };
        if cmd == SlashCommand::Goal {
            if !user_message.local_images.is_empty() || !user_message.remote_image_urls.is_empty() {
                self.add_error_message("/goal does not accept image attachments.".to_string());
                self.bottom_pane.drain_pending_submission_state();
                return true;
            }
            self.dispatch_goal_command(Some(rest.to_string()));
            return true;
        }
        if rest.trim().is_empty() {
            self.dispatch_command(cmd);
            return true;
        }
        if !user_message.local_images.is_empty() || !user_message.remote_image_urls.is_empty() {
            self.add_info_message(
                "Slash commands do not accept image attachments in the raw submission path."
                    .to_string(),
                /*hint*/ None,
            );
        }
        self.add_error_message(format!(
            "'/{name}' is a slash command. It was blocked from model input because the composer did not dispatch its inline arguments."
        ));
        self.bottom_pane.drain_pending_submission_state();
        true
    }
}

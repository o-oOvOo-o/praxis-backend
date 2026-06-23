use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_ratatui_app(
    cli: Cli,
    arg0_paths: Arg0DispatchPaths,
    loader_overrides: LoaderOverrides,
    app_gateway_target: AppGatewayTarget,
    initial_config: Config,
    initial_tui_config: TuiRuntimeConfig,
    overrides: ConfigOverrides,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    mut cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    remote_url: Option<String>,
    remote_auth_token: Option<String>,
    control_listen: Option<ControlListenConfig>,
) -> color_eyre::Result<AppExitInfo> {
    let remote_mode = matches!(&app_gateway_target, AppGatewayTarget::Remote { .. });
    install_color_eyre()?;

    // Forward panic reports through tracing so they appear in the UI status
    // line, but do not swallow the default/color-eyre panic handler.
    // Chain to the previous hook so users still get a rich panic report
    // (including backtraces) after we restore the terminal.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        prev_hook(info);
    }));
    let mut terminal = tui::init()?;
    terminal.clear()?;

    let mut tui = Tui::new(terminal);
    let mut terminal_restore_guard = TerminalRestoreGuard::new();

    #[cfg(not(debug_assertions))]
    {
        use crate::update_prompt::UpdatePromptOutcome;

        let skip_update_prompt = cli.prompt.as_ref().is_some_and(|prompt| !prompt.is_empty());
        if !skip_update_prompt {
            match update_prompt::run_update_prompt_if_needed(&mut tui, &initial_config).await? {
                UpdatePromptOutcome::Continue => {}
                UpdatePromptOutcome::RunUpdate(action) => {
                    terminal_restore_guard.restore()?;
                    return Ok(AppExitInfo {
                        token_usage: praxis_protocol::protocol::TokenUsage::default(),
                        thread_id: None,
                        thread_name: None,
                        update_action: Some(action),
                        exit_reason: ExitReason::UserRequested,
                    });
                }
            }
        }
    }

    // Initialize high-fidelity session event logging if enabled.
    session_log::maybe_init(&initial_config);

    let should_show_trust_screen_flag = !remote_mode && should_show_trust_screen(&initial_config);
    let mut trust_decision_was_made = false;
    let has_usable_non_openai_provider = has_any_usable_non_openai_provider(&initial_config);
    let needs_openai_login_status =
        initial_config.model_provider.requires_openai_auth || !has_usable_non_openai_provider;
    let needs_onboarding_app_gateway = should_show_trust_screen_flag || needs_openai_login_status;
    let mut onboarding_app_gateway = if needs_onboarding_app_gateway {
        Some(AppGatewaySession::new(
            start_app_gateway(
                &app_gateway_target,
                arg0_paths.clone(),
                initial_config.clone(),
                cli_kv_overrides.clone(),
                loader_overrides.clone(),
                cloud_requirements.clone(),
                feedback.clone(),
                None,
            )
            .await?,
        ))
    } else {
        None
    };
    let login_status = if needs_openai_login_status {
        let Some(app_gateway) = onboarding_app_gateway.as_mut() else {
            unreachable!("onboarding app gateway should exist when auth is required");
        };
        get_login_status(app_gateway, &initial_config).await?
    } else {
        LoginStatus::NotAuthenticated
    };
    let show_login_screen = should_show_login_screen(login_status, &initial_config);
    let should_show_onboarding = should_show_trust_screen_flag || show_login_screen;

    let (mut config, mut tui_config) = if should_show_onboarding {
        let onboarding_result = run_onboarding_app(
            OnboardingScreenArgs {
                show_login_screen,
                show_trust_screen: should_show_trust_screen_flag,
                login_status,
                app_gateway_request_handle: onboarding_app_gateway
                    .as_ref()
                    .map(AppGatewaySession::request_handle),
                config: initial_config.clone(),
                tui_config: initial_tui_config.clone(),
            },
            if show_login_screen {
                onboarding_app_gateway.take()
            } else {
                None
            },
            &mut tui,
        )
        .await?;
        if onboarding_result.should_exit {
            terminal_restore_guard.restore_silently();
            session_log::log_session_end();
            let _ = tui.terminal.clear();
            return Ok(AppExitInfo {
                token_usage: praxis_protocol::protocol::TokenUsage::default(),
                thread_id: None,
                thread_name: None,
                update_action: None,
                exit_reason: ExitReason::UserRequested,
            });
        }
        trust_decision_was_made = onboarding_result.directory_trust_decision.is_some();
        // If this onboarding run included the login step, always refresh cloud requirements and
        // rebuild config. This avoids missing newly available cloud requirements due to login
        // status detection edge cases.
        if show_login_screen && !remote_mode {
            cloud_requirements = cloud_config_bundle_loader_for_storage(
                initial_config.praxis_home.clone(),
                /*enable_praxis_api_key_env*/ false,
                initial_config.cli_auth_credentials_store_mode,
                initial_config.chatgpt_base_url.clone(),
            );
        }

        // If the user made an explicit trust decision, or we showed the login flow, reload config
        // so current process state reflects persisted trust/auth changes.
        if onboarding_result.directory_trust_decision.is_some()
            || (show_login_screen && !remote_mode)
        {
            let loaded = load_config_or_exit(
                cli_kv_overrides.clone(),
                overrides.clone(),
                cloud_requirements.clone(),
            )
            .await;
            (loaded.config, loaded.tui_config)
        } else {
            (initial_config, initial_tui_config)
        }
    } else {
        shutdown_app_gateway_if_present(onboarding_app_gateway.take()).await;
        (initial_config, initial_tui_config)
    };
    shutdown_app_gateway_if_present(onboarding_app_gateway.take()).await;
    if !show_login_screen
        && let Some(selection) =
            normalize_runtime_provider_model_selection(login_status, &mut config)
        && let Err(err) = ConfigEditsBuilder::new(&config.praxis_home)
            .with_profile(config.active_profile.as_deref())
            .set_model_provider(Some(selection.provider_id.as_str()))
            .set_model(Some(selection.model.as_str()), selection.effort)
            .apply()
            .await
    {
        warn!(
            error = %err,
            provider = %selection.provider_id,
            model = %selection.model,
            "failed to persist normalized provider/model selection"
        );
    }

    let mut missing_session_exit = |id_str: &str, action: &str, source: SessionLookupSource| {
        error!("Error finding conversation path: {id_str}");
        terminal_restore_guard.restore_silently();
        session_log::log_session_end();
        let _ = tui.terminal.clear();
        Ok(AppExitInfo {
            token_usage: praxis_protocol::protocol::TokenUsage::default(),
            thread_id: None,
            thread_name: None,
            update_action: None,
            exit_reason: ExitReason::Fatal(format!(
                "No saved session found with ID {id_str}. Run `{}` without an ID to choose from existing sessions.",
                session_lookup_command_hint(action, source),
            )),
        })
    };

    let needs_app_gateway_session_lookup = cli.resume_last
        || cli.fork_last
        || cli.resume_session_id.is_some()
        || cli.fork_session_id.is_some()
        || cli.resume_picker
        || cli.fork_picker;
    let session_lookup_source = if cli.fork_picker || cli.fork_last || cli.fork_session_id.is_some()
    {
        cli.fork_source
    } else {
        cli.resume_source
    };
    let session_lookup_target =
        session_lookup_app_gateway_target(session_lookup_source, &app_gateway_target);
    let mut session_lookup_context = if needs_app_gateway_session_lookup {
        Some(
            start_session_lookup_context(
                session_lookup_source,
                &config,
                &session_lookup_target,
                arg0_paths.clone(),
                loader_overrides.clone(),
                feedback.clone(),
            )
            .await?,
        )
    } else {
        None
    };

    let use_fork = cli.fork_picker || cli.fork_last || cli.fork_session_id.is_some();
    let session_selection = if use_fork {
        if let Some(id_str) = cli.fork_session_id.as_deref() {
            let lookup_source = session_lookup_context
                .as_ref()
                .map(|ctx| ctx.source)
                .expect("session lookup app gateway should be initialized for --fork <id>");
            let Some(lookup) = session_lookup_context.as_mut() else {
                unreachable!("session lookup app gateway should be initialized for --fork <id>");
            };
            match lookup_session_target_with_app_gateway(&mut lookup.app_gateway, id_str).await? {
                Some(target_session) => resume_picker::SessionSelection::Fork(target_session),
                None => {
                    shutdown_app_gateway_if_present(
                        session_lookup_context.take().map(|ctx| ctx.app_gateway),
                    )
                    .await;
                    return missing_session_exit(id_str, "fork", lookup_source);
                }
            }
        } else if cli.fork_last {
            let Some(lookup) = session_lookup_context.as_mut() else {
                unreachable!("session lookup app gateway should be initialized for --fork --last");
            };
            match lookup_latest_session_target_with_app_gateway(
                &mut lookup.app_gateway,
                /*cwd_filter*/ None,
                /*include_non_interactive*/ false,
            )
            .await?
            {
                Some(target_session) => resume_picker::SessionSelection::Fork(target_session),
                None => resume_picker::SessionSelection::StartFresh,
            }
        } else if cli.fork_picker {
            let Some(lookup) = session_lookup_context.take() else {
                unreachable!("session lookup app gateway should be initialized for --fork picker");
            };
            let SessionLookupContext {
                source,
                config: lookup_config,
                app_gateway,
            } = lookup;
            let alternate_source = if picker_source_switch_enabled(&session_lookup_target) {
                let alternate_lookup = start_session_lookup_context(
                    source.default_alternate(),
                    &config,
                    &session_lookup_target,
                    arg0_paths.clone(),
                    loader_overrides.clone(),
                    feedback.clone(),
                )
                .await?;
                Some(resume_picker::AlternatePickerSource {
                    source: alternate_lookup.source,
                    config: alternate_lookup.config,
                    app_gateway: alternate_lookup.app_gateway,
                })
            } else {
                None
            };
            match resume_picker::run_fork_picker_with_app_gateway(
                &mut tui,
                &lookup_config,
                cli.fork_show_all,
                source,
                app_gateway,
                alternate_source,
            )
            .await?
            {
                resume_picker::SessionSelection::Exit => {
                    terminal_restore_guard.restore_silently();
                    session_log::log_session_end();
                    return Ok(AppExitInfo {
                        token_usage: praxis_protocol::protocol::TokenUsage::default(),
                        thread_id: None,
                        thread_name: None,
                        update_action: None,
                        exit_reason: ExitReason::UserRequested,
                    });
                }
                other => other,
            }
        } else {
            resume_picker::SessionSelection::StartFresh
        }
    } else if let Some(id_str) = cli.resume_session_id.as_deref() {
        let lookup_source = session_lookup_context
            .as_ref()
            .map(|ctx| ctx.source)
            .expect("session lookup app gateway should be initialized for --resume <id>");
        let Some(lookup) = session_lookup_context.as_mut() else {
            unreachable!("session lookup app gateway should be initialized for --resume <id>");
        };
        match lookup_session_target_with_app_gateway(&mut lookup.app_gateway, id_str).await? {
            Some(target_session) if lookup.source.is_external() => {
                resume_picker::SessionSelection::Fork(target_session)
            }
            Some(target_session) => resume_picker::SessionSelection::Resume(target_session),
            None => {
                shutdown_app_gateway_if_present(
                    session_lookup_context.take().map(|ctx| ctx.app_gateway),
                )
                .await;
                return missing_session_exit(id_str, "resume", lookup_source);
            }
        }
    } else if cli.resume_last {
        let filter_cwd = if cli.resume_show_all {
            None
        } else {
            Some(config.cwd.as_path())
        };
        let Some(lookup) = session_lookup_context.as_mut() else {
            unreachable!("session lookup app gateway should be initialized for --resume --last");
        };
        match lookup_latest_session_target_with_app_gateway(
            &mut lookup.app_gateway,
            filter_cwd,
            cli.resume_include_non_interactive,
        )
        .await?
        {
            Some(target_session) if lookup.source.is_external() => {
                resume_picker::SessionSelection::Fork(target_session)
            }
            Some(target_session) => resume_picker::SessionSelection::Resume(target_session),
            None => resume_picker::SessionSelection::StartFresh,
        }
    } else if cli.resume_picker {
        let Some(lookup) = session_lookup_context.take() else {
            unreachable!("session lookup app gateway should be initialized for --resume picker");
        };
        let SessionLookupContext {
            source,
            config: lookup_config,
            app_gateway,
        } = lookup;
        let alternate_source = if picker_source_switch_enabled(&session_lookup_target) {
            let alternate_lookup = start_session_lookup_context(
                source.default_alternate(),
                &config,
                &session_lookup_target,
                arg0_paths.clone(),
                loader_overrides.clone(),
                feedback.clone(),
            )
            .await?;
            Some(resume_picker::AlternatePickerSource {
                source: alternate_lookup.source,
                config: alternate_lookup.config,
                app_gateway: alternate_lookup.app_gateway,
            })
        } else {
            None
        };
        match resume_picker::run_resume_picker_with_app_gateway(
            &mut tui,
            &lookup_config,
            cli.resume_show_all,
            cli.resume_include_non_interactive,
            source,
            app_gateway,
            alternate_source,
        )
        .await?
        {
            resume_picker::SessionSelection::Resume(target_session) if source.is_external() => {
                resume_picker::SessionSelection::Fork(target_session)
            }
            resume_picker::SessionSelection::Exit => {
                terminal_restore_guard.restore_silently();
                session_log::log_session_end();
                return Ok(AppExitInfo {
                    token_usage: praxis_protocol::protocol::TokenUsage::default(),
                    thread_id: None,
                    thread_name: None,
                    update_action: None,
                    exit_reason: ExitReason::UserRequested,
                });
            }
            other => other,
        }
    } else {
        resume_picker::SessionSelection::StartFresh
    };
    shutdown_app_gateway_if_present(session_lookup_context.take().map(|ctx| ctx.app_gateway)).await;

    let workspace_mode = cli.launch_mode().is_workspace();

    let current_cwd = config.cwd.clone();
    let allow_prompt = !remote_mode && cli.cwd.is_none();
    let action_and_target_session_if_resume_or_fork = match &session_selection {
        resume_picker::SessionSelection::Resume(target_session) => {
            Some((CwdPromptAction::Resume, target_session))
        }
        resume_picker::SessionSelection::Fork(target_session) => {
            Some((CwdPromptAction::Fork, target_session))
        }
        _ => None,
    };
    let fallback_cwd = match action_and_target_session_if_resume_or_fork {
        Some((action, target_session)) => {
            if remote_mode {
                Some(current_cwd.to_path_buf())
            } else {
                match resolve_cwd_for_resume_or_fork(
                    &mut tui,
                    &current_cwd,
                    target_session.cwd.as_deref(),
                    action,
                    allow_prompt,
                )
                .await?
                {
                    ResolveCwdOutcome::Continue(cwd) => cwd,
                    ResolveCwdOutcome::Exit => {
                        terminal_restore_guard.restore_silently();
                        session_log::log_session_end();
                        return Ok(AppExitInfo {
                            token_usage: praxis_protocol::protocol::TokenUsage::default(),
                            thread_id: None,
                            thread_name: None,
                            update_action: None,
                            exit_reason: ExitReason::UserRequested,
                        });
                    }
                }
            }
        }
        None => None,
    };

    let reload_for_resume = match &session_selection {
        resume_picker::SessionSelection::Resume(_) | resume_picker::SessionSelection::Fork(_) => {
            Some(
                load_config_or_exit_with_fallback_cwd(
                    cli_kv_overrides.clone(),
                    overrides.clone(),
                    cloud_requirements.clone(),
                    fallback_cwd,
                )
                .await,
            )
        }
        _ => None,
    };
    if let Some(loaded) = reload_for_resume {
        config = loaded.config;
        tui_config = loaded.tui_config;
    }

    // Configure syntax highlighting theme from the final config — onboarding
    // and resume/fork can both reload config with a different TUI theme, so
    // this must happen after the last possible reload.
    if let Some(w) = crate::render::highlight::set_theme_override(
        tui_config.theme.clone(),
        find_praxis_home().ok(),
    ) {
        config.startup_warnings.push(w);
    }

    set_default_client_residency_requirement(config.enforce_residency.value());
    let active_profile = config.active_profile.clone();
    let should_show_trust_screen = should_show_trust_screen(&config);
    let should_prompt_windows_sandbox_nux_at_startup = cfg!(target_os = "windows")
        && trust_decision_was_made
        && WindowsSandboxLevel::from_config(&config) == WindowsSandboxLevel::Disabled;

    let Cli {
        prompt,
        images,
        no_alt_screen,
        ..
    } = cli;

    let use_alt_screen =
        workspace_mode || determine_alt_screen_mode(no_alt_screen, tui_config.alternate_screen);
    tui.set_alt_screen_enabled(use_alt_screen);
    tui.set_mouse_capture_enabled(workspace_mode)?;
    let app_gateway = match start_app_gateway(
        &app_gateway_target,
        arg0_paths,
        config.clone(),
        cli_kv_overrides.clone(),
        loader_overrides,
        cloud_requirements.clone(),
        feedback.clone(),
        control_listen,
    )
    .await
    {
        Ok(app_gateway) => app_gateway,
        Err(err) => {
            terminal_restore_guard.restore_silently();
            session_log::log_session_end();
            return Err(err);
        }
    };

    if use_alt_screen {
        tui.enter_alt_screen()?;
    }

    let app_result = App::run(
        &mut tui,
        AppGatewaySession::new(app_gateway),
        config,
        tui_config,
        cli_kv_overrides.clone(),
        overrides.clone(),
        active_profile,
        prompt,
        images,
        session_selection,
        feedback,
        should_show_trust_screen, // Proxy to: is it a first run in this directory?
        should_prompt_windows_sandbox_nux_at_startup,
        remote_url,
        remote_auth_token,
        workspace_mode,
    )
    .await;

    while tui.is_alt_screen_active() {
        let _ = tui.leave_alt_screen();
    }
    terminal_restore_guard.restore_silently();
    // Mark the end of the recorded session.
    session_log::log_session_end();
    // ignore error when collecting usage – report underlying error instead
    app_result
}

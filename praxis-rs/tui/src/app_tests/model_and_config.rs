use super::*;

#[tokio::test]
async fn model_migration_prompt_only_shows_for_deprecated_models() {
    let seen = BTreeMap::new();
    assert!(should_show_model_migration_prompt(
        "gpt-5",
        "gpt-5.2-codex",
        &seen,
        &all_model_presets()
    ));
    assert!(should_show_model_migration_prompt(
        "gpt-5-codex",
        "gpt-5.2-codex",
        &seen,
        &all_model_presets()
    ));
    assert!(should_show_model_migration_prompt(
        "gpt-5-codex-mini",
        "gpt-5.2-codex",
        &seen,
        &all_model_presets()
    ));
    assert!(should_show_model_migration_prompt(
        "gpt-5.1-codex",
        "gpt-5.2-codex",
        &seen,
        &all_model_presets()
    ));
    assert!(!should_show_model_migration_prompt(
        "gpt-5.1-codex",
        "gpt-5.1-codex",
        &seen,
        &all_model_presets()
    ));
}

#[test]
fn select_model_availability_nux_picks_only_eligible_model() {
    let mut presets = all_model_presets();
    presets.iter_mut().for_each(|preset| {
        preset.availability_nux = None;
    });
    let target = presets
        .iter_mut()
        .find(|preset| preset.model == "gpt-5")
        .expect("target preset present");
    target.availability_nux = Some(ModelAvailabilityNux {
        message: "gpt-5 is available".to_string(),
    });

    let selected = select_model_availability_nux(&presets, &model_availability_nux_config(&[]));

    assert_eq!(
        selected,
        Some(StartupTooltipOverride {
            model_slug: "gpt-5".to_string(),
            message: "gpt-5 is available".to_string(),
        })
    );
}

#[test]
fn select_model_availability_nux_skips_missing_and_exhausted_models() {
    let mut presets = all_model_presets();
    presets.iter_mut().for_each(|preset| {
        preset.availability_nux = None;
    });
    let gpt_5 = presets
        .iter_mut()
        .find(|preset| preset.model == "gpt-5")
        .expect("gpt-5 preset present");
    gpt_5.availability_nux = Some(ModelAvailabilityNux {
        message: "gpt-5 is available".to_string(),
    });
    let gpt_5_2 = presets
        .iter_mut()
        .find(|preset| preset.model == "gpt-5.2")
        .expect("gpt-5.2 preset present");
    gpt_5_2.availability_nux = Some(ModelAvailabilityNux {
        message: "gpt-5.2 is available".to_string(),
    });

    let selected = select_model_availability_nux(
        &presets,
        &model_availability_nux_config(&[("gpt-5", MODEL_AVAILABILITY_NUX_MAX_SHOW_COUNT)]),
    );

    assert_eq!(
        selected,
        Some(StartupTooltipOverride {
            model_slug: "gpt-5.2".to_string(),
            message: "gpt-5.2 is available".to_string(),
        })
    );
}

#[test]
fn active_turn_not_steerable_turn_error_extracts_structured_server_error() {
    let turn_error = AppGatewayTurnError {
        message: "cannot steer a review turn".to_string(),
        praxis_error_info: Some(AppGatewayPraxisErrorInfo::ActiveTurnNotSteerable {
            turn_kind: AppGatewayNonSteerableTurnKind::Review,
        }),
        additional_details: None,
    };
    let error = TypedRequestError::Server {
        method: "turn/steer".to_string(),
        source: JSONRPCErrorError {
            code: -32602,
            message: turn_error.message.clone(),
            data: Some(serde_json::to_value(&turn_error).expect("turn error should serialize")),
        },
    };

    assert_eq!(
        active_turn_not_steerable_turn_error(&error),
        Some(turn_error)
    );
}

#[test]
fn active_turn_missing_steer_error_detects_stale_turn_race() {
    let error = TypedRequestError::Server {
        method: "turn/steer".to_string(),
        source: JSONRPCErrorError {
            code: -32602,
            message: "no active turn to steer".to_string(),
            data: None,
        },
    };

    assert!(active_turn_missing_steer_error(&error));
    assert_eq!(active_turn_not_steerable_turn_error(&error), None);
}

#[test]
fn select_model_availability_nux_uses_existing_model_order_as_priority() {
    let mut presets = all_model_presets();
    presets.iter_mut().for_each(|preset| {
        preset.availability_nux = None;
    });
    let first = presets
        .iter_mut()
        .find(|preset| preset.model == "gpt-5")
        .expect("gpt-5 preset present");
    first.availability_nux = Some(ModelAvailabilityNux {
        message: "first".to_string(),
    });
    let second = presets
        .iter_mut()
        .find(|preset| preset.model == "gpt-5.2")
        .expect("gpt-5.2 preset present");
    second.availability_nux = Some(ModelAvailabilityNux {
        message: "second".to_string(),
    });

    let selected = select_model_availability_nux(&presets, &model_availability_nux_config(&[]));

    assert_eq!(
        selected,
        Some(StartupTooltipOverride {
            model_slug: "gpt-5.2".to_string(),
            message: "second".to_string(),
        })
    );
}

#[test]
fn select_model_availability_nux_returns_none_when_all_models_are_exhausted() {
    let mut presets = all_model_presets();
    presets.iter_mut().for_each(|preset| {
        preset.availability_nux = None;
    });
    let target = presets
        .iter_mut()
        .find(|preset| preset.model == "gpt-5")
        .expect("target preset present");
    target.availability_nux = Some(ModelAvailabilityNux {
        message: "gpt-5 is available".to_string(),
    });

    let selected = select_model_availability_nux(
        &presets,
        &model_availability_nux_config(&[("gpt-5", MODEL_AVAILABILITY_NUX_MAX_SHOW_COUNT)]),
    );

    assert_eq!(selected, None);
}

#[tokio::test]
async fn model_migration_prompt_respects_hide_flag_and_self_target() {
    let mut seen = BTreeMap::new();
    seen.insert("gpt-5".to_string(), "gpt-5.1".to_string());
    assert!(!should_show_model_migration_prompt(
        "gpt-5",
        "gpt-5.1",
        &seen,
        &all_model_presets()
    ));
    assert!(!should_show_model_migration_prompt(
        "gpt-5.1",
        "gpt-5.1",
        &seen,
        &all_model_presets()
    ));
}

#[tokio::test]
async fn model_migration_prompt_skips_when_target_missing_or_hidden() {
    let mut available = all_model_presets();
    let mut current = available
        .iter()
        .find(|preset| preset.model == "gpt-5-codex")
        .cloned()
        .expect("preset present");
    current.upgrade = Some(ModelUpgrade {
        id: "missing-target".to_string(),
        reasoning_effort_mapping: None,
        migration_config_key: HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG.to_string(),
        model_link: None,
        upgrade_copy: None,
        migration_markdown: None,
    });
    available.retain(|preset| preset.model != "gpt-5-codex");
    available.push(current.clone());

    assert!(!should_show_model_migration_prompt(
        &current.model,
        "missing-target",
        &BTreeMap::new(),
        &available,
    ));

    assert!(target_preset_for_upgrade(&available, "missing-target").is_none());

    let mut with_hidden_target = all_model_presets();
    let target = with_hidden_target
        .iter_mut()
        .find(|preset| preset.model == "gpt-5.2-codex")
        .expect("target preset present");
    target.show_in_picker = false;

    assert!(!should_show_model_migration_prompt(
        "gpt-5-codex",
        "gpt-5.2-codex",
        &BTreeMap::new(),
        &with_hidden_target,
    ));
    assert!(target_preset_for_upgrade(&with_hidden_target, "gpt-5.2-codex").is_none());
}

#[tokio::test]
async fn model_migration_prompt_shows_for_hidden_model() {
    let praxis_home = tempdir().expect("temp Praxis home");
    let config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .build()
        .await
        .expect("config");

    let mut available_models = all_model_presets();
    let current = available_models
        .iter()
        .find(|preset| preset.model == "gpt-5.1-codex")
        .cloned()
        .expect("gpt-5.1-codex preset present");
    assert!(
        !current.show_in_picker,
        "expected gpt-5.1-codex to be hidden from picker for this test"
    );

    let upgrade = current.upgrade.as_ref().expect("upgrade configured");
    // Test "hidden current model still prompts" even if bundled
    // catalog data changes the target model's picker visibility.
    available_models
        .iter_mut()
        .find(|preset| preset.model == upgrade.id)
        .expect("upgrade target present")
        .show_in_picker = true;
    assert!(
        should_show_model_migration_prompt(
            &current.model,
            &upgrade.id,
            &config.notices.model_migrations,
            &available_models,
        ),
        "expected migration prompt to be eligible for hidden model"
    );

    let target =
        target_preset_for_upgrade(&available_models, &upgrade.id).expect("upgrade target present");
    let target_description = (!target.description.is_empty()).then(|| target.description.clone());
    let can_opt_out = true;
    let copy = migration_copy_for_models(
        &current.model,
        &upgrade.id,
        upgrade.model_link.clone(),
        upgrade.upgrade_copy.clone(),
        upgrade.migration_markdown.clone(),
        target.display_name.clone(),
        target_description,
        can_opt_out,
    );

    // Snapshot the copy we would show; rendering is covered by model_migration snapshots.
    assert_snapshot!(
        "model_migration_prompt_shows_for_hidden_model",
        model_migration_copy_to_plain_text(&copy)
    );
}

#[tokio::test]
async fn update_reasoning_effort_updates_collaboration_mode() {
    let mut app = make_test_app().await;
    app.chat_widget
        .set_reasoning_effort(Some(ReasoningEffortConfig::Medium));

    app.on_update_reasoning_effort(Some(ReasoningEffortConfig::High));

    assert_eq!(
        app.chat_widget.current_reasoning_effort(),
        Some(ReasoningEffortConfig::High)
    );
    assert_eq!(
        app.config.model_reasoning_effort,
        Some(ReasoningEffortConfig::High)
    );
}

#[tokio::test]
async fn refresh_in_memory_config_from_disk_loads_latest_apps_state() -> Result<()> {
    let mut app = make_test_app().await;
    let praxis_home = tempdir()?;
    app.config.praxis_home = praxis_home.path().to_path_buf();
    let app_id = "unit_test_refresh_in_memory_config_connector".to_string();

    assert_eq!(app_enabled_in_effective_config(&app.config, &app_id), None);

    ConfigEditsBuilder::new(&app.config.praxis_home)
        .with_edits([
            ConfigEdit::SetPath {
                segments: vec!["apps".to_string(), app_id.clone(), "enabled".to_string()],
                value: false.into(),
            },
            ConfigEdit::SetPath {
                segments: vec![
                    "apps".to_string(),
                    app_id.clone(),
                    "disabled_reason".to_string(),
                ],
                value: "user".into(),
            },
        ])
        .apply()
        .await
        .expect("persist app toggle");

    assert_eq!(app_enabled_in_effective_config(&app.config, &app_id), None);

    app.refresh_in_memory_config_from_disk().await?;

    assert_eq!(
        app_enabled_in_effective_config(&app.config, &app_id),
        Some(false)
    );
    Ok(())
}

#[tokio::test]
async fn refresh_in_memory_config_from_disk_best_effort_keeps_current_config_on_error() -> Result<()>
{
    let mut app = make_test_app().await;
    let praxis_home = tempdir()?;
    app.config.praxis_home = praxis_home.path().to_path_buf();
    std::fs::write(praxis_home.path().join("config.toml"), "[broken")?;
    let original_config = app.config.clone();

    app.refresh_in_memory_config_from_disk_best_effort("starting a new thread")
        .await;

    assert_eq!(app.config, original_config);
    Ok(())
}

#[tokio::test]
async fn refresh_in_memory_config_from_disk_uses_active_chat_widget_cwd() -> Result<()> {
    let mut app = make_test_app().await;
    let original_cwd = app.config.cwd.clone();
    let next_cwd_tmp = tempdir()?;
    let next_cwd = next_cwd_tmp.path().to_path_buf();

    app.chat_widget.handle_praxis_event(Event {
        id: String::new(),
        msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: ThreadId::new(),
            forked_from_id: None,
            thread_name: None,
            model: "gpt-test".to_string(),
            model_provider_id: "test-provider".to_string(),
            service_tier: None,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            cwd: next_cwd.clone(),
            reasoning_effort: None,
            history_log_id: 0,
            history_entry_count: 0,
            initial_messages: None,
            network_proxy: None,
            rollout_path: Some(PathBuf::new()),
        }),
    });

    assert_eq!(app.chat_widget.config_ref().cwd.to_path_buf(), next_cwd);
    assert_eq!(app.config.cwd, original_cwd);

    app.refresh_in_memory_config_from_disk().await?;

    assert_eq!(app.config.cwd, app.chat_widget.config_ref().cwd);
    Ok(())
}

#[tokio::test]
async fn rebuild_config_for_resume_or_fallback_uses_current_config_on_same_cwd_error() -> Result<()>
{
    let mut app = make_test_app().await;
    let praxis_home = tempdir()?;
    app.config.praxis_home = praxis_home.path().to_path_buf();
    std::fs::write(praxis_home.path().join("config.toml"), "[broken")?;
    let current_config = app.config.clone();
    let current_cwd = current_config.cwd.clone();

    let (resume_config, resume_tui_config) = app
        .rebuild_config_for_resume_or_fallback(&current_cwd, current_cwd.to_path_buf())
        .await?;

    assert_eq!(resume_config, current_config);
    assert_eq!(resume_tui_config, app.tui_config);
    Ok(())
}

#[tokio::test]
async fn rebuild_config_for_resume_or_fallback_errors_when_cwd_changes() -> Result<()> {
    let mut app = make_test_app().await;
    let praxis_home = tempdir()?;
    app.config.praxis_home = praxis_home.path().to_path_buf();
    std::fs::write(praxis_home.path().join("config.toml"), "[broken")?;
    let current_cwd = app.config.cwd.clone();
    let next_cwd_tmp = tempdir()?;
    let next_cwd = next_cwd_tmp.path().to_path_buf();

    let result = app
        .rebuild_config_for_resume_or_fallback(&current_cwd, next_cwd)
        .await;

    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn sync_tui_theme_selection_updates_chat_widget_config_copy() {
    let mut app = make_test_app().await;

    app.sync_tui_theme_selection("dracula".to_string());

    assert_eq!(app.tui_config.theme.as_deref(), Some("dracula"));
    assert_eq!(
        app.chat_widget.tui_config_ref().theme.as_deref(),
        Some("dracula")
    );
}

#[tokio::test]
async fn fresh_session_config_uses_current_service_tier() {
    let mut app = make_test_app().await;
    app.chat_widget
        .set_service_tier(Some(praxis_protocol::config_types::ServiceTier::Fast));

    let config = app.fresh_session_config();

    assert_eq!(
        config.service_tier,
        Some(praxis_protocol::config_types::ServiceTier::Fast)
    );
}

use super::*;

#[tokio::test]
async fn open_agent_picker_keeps_missing_threads_for_replay() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let thread_id = ThreadId::new();
    app.thread_event_channels
        .insert(thread_id, ThreadEventChannel::new(/*capacity*/ 1));

    app.open_agent_picker(&mut app_gateway).await;

    assert_eq!(app.thread_event_channels.contains_key(&thread_id), true);
    assert_eq!(
        app.agent_navigation.get(&thread_id),
        Some(&AgentPickerThreadEntry {
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
            is_closed: true,
        })
    );
    assert_eq!(app.agent_navigation.ordered_thread_ids(), vec![thread_id]);
    Ok(())
}

#[tokio::test]
async fn open_agent_picker_preserves_cached_metadata_for_replay_threads() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let thread_id = ThreadId::new();
    app.thread_event_channels
        .insert(thread_id, ThreadEventChannel::new(/*capacity*/ 1));
    app.agent_navigation.upsert(
        thread_id,
        Some("墨子".to_string()),
        Some("巡检仓库".to_string()),
        Some("Robie".to_string()),
        Some("explorer".to_string()),
        /*is_closed*/ true,
    );

    app.open_agent_picker(&mut app_gateway).await;

    assert_eq!(app.thread_event_channels.contains_key(&thread_id), true);
    assert_eq!(
        app.agent_navigation.get(&thread_id),
        Some(&AgentPickerThreadEntry {
            agent_base_name: Some("墨子".to_string()),
            agent_title: Some("巡检仓库".to_string()),
            agent_display_name: Some("Robie".to_string()),
            agent_role: Some("explorer".to_string()),
            is_closed: true,
        })
    );
    Ok(())
}

#[tokio::test]
async fn open_agent_picker_prunes_terminal_metadata_only_threads() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let thread_id = ThreadId::new();
    app.agent_navigation.upsert(
        thread_id,
        /*agent_base_name*/ None,
        /*agent_title*/ None,
        Some("Ghost".to_string()),
        Some("worker".to_string()),
        /*is_closed*/ false,
    );

    app.open_agent_picker(&mut app_gateway).await;

    assert_eq!(app.agent_navigation.get(&thread_id), None);
    assert!(app.agent_navigation.is_empty());
    Ok(())
}

#[tokio::test]
async fn open_agent_picker_marks_terminal_read_errors_closed() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let thread_id = ThreadId::new();
    app.thread_event_channels
        .insert(thread_id, ThreadEventChannel::new(/*capacity*/ 1));
    app.agent_navigation.upsert(
        thread_id,
        Some("墨子".to_string()),
        Some("巡检仓库".to_string()),
        Some("Robie".to_string()),
        Some("explorer".to_string()),
        /*is_closed*/ false,
    );

    app.open_agent_picker(&mut app_gateway).await;

    assert_eq!(
        app.agent_navigation.get(&thread_id),
        Some(&AgentPickerThreadEntry {
            agent_base_name: Some("墨子".to_string()),
            agent_title: Some("巡检仓库".to_string()),
            agent_display_name: Some("Robie".to_string()),
            agent_role: Some("explorer".to_string()),
            is_closed: true,
        })
    );
    Ok(())
}

#[test]
fn terminal_thread_read_error_detection_matches_not_loaded_errors() {
    let err = color_eyre::eyre::eyre!(
        "thread/read failed during TUI session lookup: thread/read failed: thread not loaded: thr_123"
    );

    assert!(App::is_terminal_thread_read_error(&err));
}

#[test]
fn terminal_thread_read_error_detection_ignores_transient_failures() {
    let err = color_eyre::eyre::eyre!(
        "thread/read failed during TUI session lookup: thread/read transport error: broken pipe"
    );

    assert!(!App::is_terminal_thread_read_error(&err));
}

#[test]
fn closed_state_for_thread_read_error_preserves_live_state_without_cache_on_transient_error() {
    let err = color_eyre::eyre::eyre!(
        "thread/read failed during TUI session lookup: thread/read transport error: broken pipe"
    );

    assert!(!App::closed_state_for_thread_read_error(
        &err, /*existing_is_closed*/ None
    ));
}

#[test]
fn closed_state_for_thread_read_error_marks_terminal_uncached_threads_closed() {
    let err = color_eyre::eyre::eyre!(
        "thread/read failed during TUI session lookup: thread/read failed: thread not loaded: thr_123"
    );

    assert!(App::closed_state_for_thread_read_error(
        &err, /*existing_is_closed*/ None
    ));
}

#[test]
fn include_turns_fallback_detection_handles_unmaterialized_and_ephemeral_threads() {
    let unmaterialized = color_eyre::eyre::eyre!(
        "thread/read failed during TUI session lookup: thread/read failed: thread thr_123 is not materialized yet; includeTurns is unavailable before first user message"
    );
    let ephemeral = color_eyre::eyre::eyre!(
        "thread/read failed during TUI session lookup: thread/read failed: ephemeral threads do not support includeTurns"
    );

    assert!(App::can_fallback_from_include_turns_error(&unmaterialized));
    assert!(App::can_fallback_from_include_turns_error(&ephemeral));
}

#[tokio::test]
async fn open_agent_picker_marks_loaded_threads_open() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let started = app_gateway
        .start_thread(app.chat_widget.config_ref())
        .await?;
    let thread_id = started.session.thread_id;
    app.thread_event_channels
        .insert(thread_id, ThreadEventChannel::new(/*capacity*/ 1));

    app.open_agent_picker(&mut app_gateway).await;

    assert_eq!(
        app.agent_navigation.get(&thread_id),
        Some(&AgentPickerThreadEntry {
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
            is_closed: false,
        })
    );
    Ok(())
}

#[tokio::test]
async fn attach_live_thread_for_selection_rejects_empty_non_ephemeral_fallback_threads()
-> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let started = app_gateway
        .start_thread(app.chat_widget.config_ref())
        .await?;
    let thread_id = started.session.thread_id;
    app.agent_navigation.upsert(
        thread_id,
        Some("墨子".to_string()),
        Some("接管空线程".to_string()),
        Some("Scout".to_string()),
        Some("worker".to_string()),
        /*is_closed*/ false,
    );

    let err = app
        .attach_live_thread_for_selection(&mut app_gateway, thread_id)
        .await
        .expect_err("empty fallback should not attach as a blank replay-only thread");

    assert_eq!(
        err.to_string(),
        format!("Agent thread {thread_id} is not yet available for replay or live attach.")
    );
    assert!(!app.thread_event_channels.contains_key(&thread_id));
    Ok(())
}

#[tokio::test]
async fn attach_live_thread_for_selection_rejects_unmaterialized_fallback_threads() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let mut ephemeral_config = app.chat_widget.config_ref().clone();
    ephemeral_config.ephemeral = true;
    let started = app_gateway.start_thread(&ephemeral_config).await?;
    let thread_id = started.session.thread_id;
    app.agent_navigation.upsert(
        thread_id,
        Some("墨子".to_string()),
        Some("接管临时线程".to_string()),
        Some("Scout".to_string()),
        Some("worker".to_string()),
        /*is_closed*/ false,
    );

    let err = app
        .attach_live_thread_for_selection(&mut app_gateway, thread_id)
        .await
        .expect_err("ephemeral fallback should not attach as a blank live thread");

    assert_eq!(
        err.to_string(),
        format!("Agent thread {thread_id} is not yet available for replay or live attach.")
    );
    assert!(!app.thread_event_channels.contains_key(&thread_id));
    Ok(())
}

#[tokio::test]
async fn should_attach_live_thread_for_selection_skips_closed_metadata_only_threads() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    app.agent_navigation.upsert(
        thread_id,
        /*agent_base_name*/ None,
        /*agent_title*/ None,
        Some("Ghost".to_string()),
        Some("worker".to_string()),
        /*is_closed*/ true,
    );

    assert!(!app.should_attach_live_thread_for_selection(thread_id));

    app.agent_navigation.upsert(
        thread_id,
        /*agent_base_name*/ None,
        /*agent_title*/ None,
        Some("Ghost".to_string()),
        Some("worker".to_string()),
        /*is_closed*/ false,
    );
    assert!(app.should_attach_live_thread_for_selection(thread_id));

    app.thread_event_channels
        .insert(thread_id, ThreadEventChannel::new(/*capacity*/ 1));
    assert!(!app.should_attach_live_thread_for_selection(thread_id));
}

#[tokio::test]
async fn refresh_agent_picker_thread_liveness_prunes_closed_metadata_only_threads() -> Result<()> {
    let mut app = make_test_app().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let thread_id = ThreadId::new();
    app.agent_navigation.upsert(
        thread_id,
        /*agent_base_name*/ None,
        /*agent_title*/ None,
        Some("Ghost".to_string()),
        Some("worker".to_string()),
        /*is_closed*/ false,
    );

    let is_available = app
        .refresh_agent_picker_thread_liveness(&mut app_gateway, thread_id)
        .await;

    assert!(!is_available);
    assert_eq!(app.agent_navigation.get(&thread_id), None);
    assert!(!app.thread_event_channels.contains_key(&thread_id));
    Ok(())
}

#[tokio::test]
async fn open_agent_picker_prompts_to_enable_multi_agent_when_disabled() -> Result<()> {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let _ = app.config.features.disable(Feature::Collab);

    app.open_agent_picker(&mut app_gateway).await;
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_matches!(
        app_event_rx.try_recv(),
        Ok(AppEvent::UpdateFeatureFlags { updates }) if updates == vec![(Feature::Collab, true)]
    );
    let cell = match app_event_rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => cell,
        other => panic!("expected InsertHistoryCell event, got {other:?}"),
    };
    let rendered = cell
        .display_lines(/*width*/ 120)
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Subagents will be enabled in the next session."));
    Ok(())
}

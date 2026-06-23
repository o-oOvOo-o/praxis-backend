use super::*;

#[tokio::test]
async fn status_line_invalid_items_warn_once() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.tui_config.status_line = Some(vec![
        "model_name".to_string(),
        "bogus_item".to_string(),
        "lines_changed".to_string(),
        "bogus_item".to_string(),
    ]);
    chat.thread_id = Some(ThreadId::new());

    chat.refresh_status_line();
    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one warning history cell");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("bogus_item"),
        "warning cell missing invalid item content: {rendered}"
    );

    chat.refresh_status_line();
    let cells = drain_insert_history(&mut rx);
    assert!(
        cells.is_empty(),
        "expected invalid status line warning to emit only once"
    );
}

#[tokio::test]
async fn status_line_branch_state_resets_when_git_branch_disabled() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.status_line_branch = Some("main".to_string());
    chat.status_line_branch_pending = true;
    chat.status_line_branch_lookup_complete = true;
    chat.tui_config.status_line = Some(vec!["model_name".to_string()]);

    chat.refresh_status_line();

    assert_eq!(chat.status_line_branch, None);
    assert!(!chat.status_line_branch_pending);
    assert!(!chat.status_line_branch_lookup_complete);
}

#[tokio::test]
async fn status_line_branch_refreshes_after_turn_complete() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.tui_config.status_line = Some(vec!["git-branch".to_string()]);
    chat.status_line_branch_lookup_complete = true;
    chat.status_line_branch_pending = false;

    chat.handle_praxis_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
        }),
    });

    assert!(chat.status_line_branch_pending);
}

#[tokio::test]
async fn status_line_branch_refreshes_after_interrupt() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.tui_config.status_line = Some(vec!["git-branch".to_string()]);
    chat.status_line_branch_lookup_complete = true;
    chat.status_line_branch_pending = false;

    chat.handle_praxis_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnAborted(praxis_protocol::protocol::TurnAbortedEvent {
            turn_id: Some("turn-1".to_string()),
            reason: TurnAbortReason::Interrupted,
        }),
    });

    assert!(chat.status_line_branch_pending);
}

#[tokio::test]
async fn status_line_fast_mode_renders_on_and_off() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.tui_config.status_line = Some(vec!["fast-mode".to_string()]);

    chat.refresh_status_line();
    assert_eq!(status_line_text(&chat), Some("Fast off".to_string()));

    chat.set_service_tier(Some(ServiceTier::Fast));
    chat.refresh_status_line();
    assert_eq!(status_line_text(&chat), Some("Fast on".to_string()));
}

#[tokio::test]
async fn status_line_renders_above_footer_instead_of_replacing_it() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    chat.config.cwd = test_project_path().abs();
    chat.tui_config.status_line = Some(vec!["model-with-reasoning".to_string()]);
    chat.refresh_status_line();

    let status_line = status_line_text(&chat).expect("status line should be populated");
    let rendered = normalize_snapshot_paths(render_bottom_popup(&chat, /*width*/ 100));
    let lines = rendered.lines().collect::<Vec<_>>();
    let status_line_index = lines
        .iter()
        .position(|line| line.contains(&status_line))
        .expect("status line row should be rendered");
    let footer_index = lines
        .iter()
        .position(|line| line.contains("shortcuts"))
        .expect("footer row should still be rendered");

    assert!(
        status_line_index < footer_index,
        "expected status line above footer, rendered:\n{rendered}"
    );
}

#[tokio::test]
async fn status_line_fast_mode_footer_snapshot() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.show_welcome_banner = false;
    chat.tui_config.status_line = Some(vec!["fast-mode".to_string()]);
    chat.set_service_tier(Some(ServiceTier::Fast));
    chat.refresh_status_line();

    let width = 80;
    let height = chat.desired_height(width);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw fast-mode footer");
    assert_chatwidget_snapshot!(
        "status_line_fast_mode_footer",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn status_line_model_with_reasoning_includes_fast_for_gpt54_only() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    chat.config.cwd = test_project_path().abs();
    chat.tui_config.status_line = Some(vec![
        "model-with-reasoning".to_string(),
        "context-remaining".to_string(),
        "current-dir".to_string(),
    ]);
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::XHigh));
    chat.set_service_tier(Some(ServiceTier::Fast));
    set_chatgpt_auth(&mut chat);
    chat.refresh_status_line();
    let test_cwd = test_path_display("/tmp/project");

    assert_eq!(
        status_line_text(&chat),
        Some(format!("gpt-5.4 xhigh fast · 100% left · {test_cwd}"))
    );

    chat.set_model("gpt-5.3-codex");
    chat.refresh_status_line();

    assert_eq!(
        status_line_text(&chat),
        Some(format!("gpt-5.3-codex xhigh · 100% left · {test_cwd}"))
    );
}

#[tokio::test]
async fn terminal_title_model_updates_on_model_change_without_manual_refresh() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    chat.tui_config.terminal_title = Some(vec!["model".to_string()]);
    chat.refresh_terminal_title();

    assert_eq!(chat.last_terminal_title, Some("gpt-5.4".to_string()));

    chat.set_model("gpt-5.3-codex");

    assert_eq!(chat.last_terminal_title, Some("gpt-5.3-codex".to_string()));
}

#[tokio::test]
async fn terminal_title_defaults_to_project_and_thread_name() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.tui_config.terminal_title = None;
    chat.config.cwd = test_project_path().abs();
    chat.thread_name = Some("Investigate flaky test".to_string());

    chat.refresh_terminal_title();

    assert_eq!(
        chat.last_terminal_title,
        Some("project · Investigate flaky test".to_string())
    );
}

#[tokio::test]
async fn terminal_title_default_omits_thread_separator_until_name_exists() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.tui_config.terminal_title = None;
    chat.config.cwd = test_project_path().abs();
    chat.thread_name = None;

    chat.refresh_terminal_title();

    assert_eq!(chat.last_terminal_title, Some("project".to_string()));
}

#[tokio::test]
async fn terminal_title_default_prefers_current_directory_name() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.tui_config.terminal_title = None;
    chat.config.cwd = PathBuf::from(test_path_display("/tmp/project/subdir")).abs();
    chat.thread_name = Some("Investigate flaky test".to_string());

    chat.refresh_terminal_title();

    assert_eq!(
        chat.last_terminal_title,
        Some("subdir · Investigate flaky test".to_string())
    );
}

#[tokio::test]
async fn status_line_model_with_reasoning_updates_on_mode_switch_without_manual_refresh() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-codex")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    chat.tui_config.status_line = Some(vec!["model-with-reasoning".to_string()]);
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::High));

    assert_eq!(
        status_line_text(&chat),
        Some("gpt-5.3-codex high".to_string())
    );

    let plan_mask = collaboration_modes::plan_mask(chat.model_catalog.as_ref())
        .expect("expected plan collaboration mode");
    chat.set_collaboration_mask(plan_mask);

    assert_eq!(
        status_line_text(&chat),
        Some("gpt-5.3-codex medium".to_string())
    );

    let default_mask = collaboration_modes::default_mask(chat.model_catalog.as_ref())
        .expect("expected default collaboration mode");
    chat.set_collaboration_mask(default_mask);

    assert_eq!(
        status_line_text(&chat),
        Some("gpt-5.3-codex high".to_string())
    );
}

#[tokio::test]
async fn status_line_model_with_reasoning_plan_mode_footer_snapshot() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-codex")).await;
    chat.show_welcome_banner = false;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    chat.tui_config.status_line = Some(vec!["model-with-reasoning".to_string()]);
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::High));

    let plan_mask = collaboration_modes::plan_mask(chat.model_catalog.as_ref())
        .expect("expected plan collaboration mode");
    chat.set_collaboration_mask(plan_mask);

    let width = 80;
    let height = chat.desired_height(width);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw plan-mode footer");
    assert_chatwidget_snapshot!(
        "status_line_model_with_reasoning_plan_mode_footer",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn status_line_model_with_reasoning_fast_footer_snapshot() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    chat.show_welcome_banner = false;
    chat.config.cwd = test_project_path().abs();
    chat.tui_config.status_line = Some(vec![
        "model-with-reasoning".to_string(),
        "context-remaining".to_string(),
        "current-dir".to_string(),
    ]);
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::XHigh));
    chat.set_service_tier(Some(ServiceTier::Fast));
    set_chatgpt_auth(&mut chat);
    chat.refresh_status_line();

    let width = 80;
    let height = chat.desired_height(width);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw model-with-reasoning footer");
    assert_chatwidget_snapshot!(
        "status_line_model_with_reasoning_fast_footer",
        normalized_backend_snapshot(terminal.backend())
    );
}

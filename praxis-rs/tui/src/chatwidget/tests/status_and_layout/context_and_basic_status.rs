use super::*;

/// Receiving a TokenCount event without usage clears the context indicator.
#[tokio::test]
async fn token_count_none_resets_context_indicator() {
    let (mut chat, _rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;

    let context_window = 13_000;
    let pre_compact_tokens = 12_700;

    chat.handle_praxis_event(Event {
        id: "token-before".into(),
        msg: EventMsg::TokenCount(TokenCountEvent {
            info: Some(make_token_info(pre_compact_tokens, context_window)),
            rate_limits: None,
        }),
    });
    assert_eq!(chat.bottom_pane.context_window_percent(), Some(30));

    chat.handle_praxis_event(Event {
        id: "token-cleared".into(),
        msg: EventMsg::TokenCount(TokenCountEvent {
            info: None,
            rate_limits: None,
        }),
    });
    assert_eq!(chat.bottom_pane.context_window_percent(), None);
}

#[tokio::test]
async fn context_indicator_shows_used_tokens_when_window_unknown() {
    let (mut chat, _rx, _ops) = make_chatwidget_manual(Some("unknown-model")).await;

    chat.config.model_context_window = None;
    let auto_compact_limit = 200_000;
    chat.config.model_auto_compact_token_limit = Some(auto_compact_limit);

    // No model window, so the indicator should fall back to showing tokens used.
    let total_tokens = 106_000;
    let token_usage = TokenUsage {
        total_tokens,
        ..TokenUsage::default()
    };
    let token_info = TokenUsageInfo {
        total_token_usage: token_usage.clone(),
        last_token_usage: token_usage,
        model_context_window: None,
        model_auto_compact_token_limit: Some(auto_compact_limit),
    };

    chat.handle_praxis_event(Event {
        id: "token-usage".into(),
        msg: EventMsg::TokenCount(TokenCountEvent {
            info: Some(token_info),
            rate_limits: None,
        }),
    });

    assert_eq!(chat.bottom_pane.context_window_percent(), None);
    assert_eq!(
        chat.bottom_pane.context_window_used_tokens(),
        Some(total_tokens)
    );
}

#[tokio::test]
async fn turn_started_uses_runtime_context_window_before_first_token_count() {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.config.model_context_window = Some(1_000_000);

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: Some(950_000),
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    assert_eq!(
        chat.status_line_value_for_item(&crate::bottom_pane::StatusLineItem::ContextWindowSize),
        Some("950K window".to_string())
    );
    assert_eq!(chat.bottom_pane.context_window_percent(), Some(100));

    chat.add_status_output(
        /*refreshing_rate_limits*/ false, /*request_id*/ None,
    );

    let cells = drain_insert_history(&mut rx);
    let context_line = cells
        .last()
        .expect("status output inserted")
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .find(|line| line.contains("Context window"))
        .expect("context window line");

    assert!(
        context_line.contains("950K"),
        "expected /status to use TurnStarted context window, got: {context_line}"
    );
    assert!(
        !context_line.contains("1M"),
        "expected /status to avoid raw config context window, got: {context_line}"
    );
}

#[tokio::test]
async fn running_status_footer_shows_context_budget_line() {
    let (mut chat, _rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.config.model_auto_compact_token_limit = Some(200_000);
    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: Some(950_000),
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    chat.handle_praxis_event(Event {
        id: "token-usage".into(),
        msg: EventMsg::TokenCount(TokenCountEvent {
            info: Some(make_token_info(127_000, 950_000)),
            rate_limits: None,
        }),
    });

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(
        status.footer_lines(),
        &["Context: 127K / 200K (64%)".to_string()]
    );
}

#[tokio::test]
async fn helpers_are_available_and_do_not_panic() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let cfg = test_config().await;
    let resolved_model = praxis_core::test_support::get_model_offline(cfg.model.as_deref());
    let session_telemetry = test_session_telemetry(&cfg, resolved_model.as_str());
    let init = ChatWidgetInit {
        config: cfg.clone(),
        tui_config: TuiRuntimeConfig::default(),
        frame_requester: FrameRequester::test_dummy(),
        app_event_tx: tx,
        initial_user_message: None,
        enhanced_keys_supported: false,
        has_chatgpt_account: false,
        model_catalog: test_model_catalog(&cfg),
        feedback: praxis_feedback::PraxisFeedback::new(),
        is_first_run: true,
        status_account_display: None,
        initial_plan_type: None,
        model: Some(resolved_model),
        startup_tooltip_override: None,
        status_line_invalid_items_warned: Arc::new(AtomicBool::new(false)),
        terminal_title_invalid_items_warned: Arc::new(AtomicBool::new(false)),
        session_telemetry,
    };
    let mut w = ChatWidget::new_with_app_event(init);
    // Basic construction sanity.
    let _ = &mut w;
}

#[tokio::test]
async fn prefetch_rate_limits_is_gated_on_chatgpt_auth_provider() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    assert!(!chat.should_prefetch_rate_limits());

    set_chatgpt_auth(&mut chat);
    assert!(chat.should_prefetch_rate_limits());

    chat.config.model_provider.requires_openai_auth = false;
    assert!(!chat.should_prefetch_rate_limits());

    chat.prefetch_rate_limits();
    assert!(!chat.should_prefetch_rate_limits());
}

#[tokio::test]
async fn worked_elapsed_from_resets_when_timer_restarts() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    assert_eq!(chat.worked_elapsed_from(/*current_elapsed*/ 5), 5);
    assert_eq!(chat.worked_elapsed_from(/*current_elapsed*/ 9), 4);
    // Simulate status timer resetting (e.g., status indicator recreated for a new task).
    assert_eq!(chat.worked_elapsed_from(/*current_elapsed*/ 3), 3);
    assert_eq!(chat.worked_elapsed_from(/*current_elapsed*/ 7), 4);
}

#[tokio::test]
async fn in_app_toast_reserves_a_row_above_bottom_pane() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let width: u16 = 80;
    let base_height = chat.desired_height(width);

    chat.show_info_toast("Saved");

    assert_eq!(chat.desired_height(width), base_height + 1);
}

#[tokio::test]
async fn status_activity_trail_omits_recent_prefix() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    chat.push_status_activity("Apply patch");
    chat.push_status_activity("Web search");
    chat.push_status_activity("ls");

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(
        status.activity_message(),
        Some("Apply patch → Web search → ls")
    );
}

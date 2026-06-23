use super::*;

#[tokio::test]
async fn realtime_error_closes_without_followup_closed_info() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.realtime_conversation.phase = RealtimeConversationPhase::Active;

    chat.on_realtime_conversation_realtime(RealtimeConversationRealtimeEvent {
        payload: RealtimeEvent::Error("boom".to_string()),
    });
    next_realtime_close_op(&mut op_rx);

    chat.on_realtime_conversation_closed(RealtimeConversationClosedEvent {
        reason: Some("error".to_string()),
    });

    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>();
    insta::assert_snapshot!(rendered.join("\n\n"), @"■ Realtime voice error: boom");
}

#[cfg(not(target_os = "linux"))]
#[tokio::test]
async fn deleted_realtime_meter_uses_shared_stop_path() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.realtime_conversation.phase = RealtimeConversationPhase::Active;
    let placeholder_id = chat.bottom_pane.insert_recording_meter_placeholder("⠤⠤⠤⠤");
    chat.realtime_conversation.meter_placeholder_id = Some(placeholder_id.clone());

    assert!(chat.stop_realtime_conversation_for_deleted_meter(&placeholder_id));

    next_realtime_close_op(&mut op_rx);
    assert_eq!(chat.realtime_conversation.meter_placeholder_id, None);
    assert_eq!(
        chat.realtime_conversation.phase,
        RealtimeConversationPhase::Stopping
    );
}

#[tokio::test]
async fn experimental_mode_plan_is_ignored_on_startup() {
    let praxis_home = tempdir().expect("tempdir");
    let cfg = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .cli_overrides(vec![
            (
                "features.collaboration_modes".to_string(),
                TomlValue::Boolean(true),
            ),
            (
                "tui.experimental_mode".to_string(),
                TomlValue::String("plan".to_string()),
            ),
        ])
        .build()
        .await
        .expect("config");
    let resolved_model = praxis_core::test_support::get_model_offline(cfg.model.as_deref());
    let session_telemetry = test_session_telemetry(&cfg, resolved_model.as_str());
    let init = ChatWidgetInit {
        config: cfg.clone(),
        tui_config: TuiRuntimeConfig::default(),
        frame_requester: FrameRequester::test_dummy(),
        app_event_tx: AppEventSender::new(unbounded_channel::<AppEvent>().0),
        initial_user_message: None,
        enhanced_keys_supported: false,
        has_chatgpt_account: false,
        model_catalog: test_model_catalog(&cfg),
        feedback: praxis_feedback::PraxisFeedback::new(),
        is_first_run: true,
        status_account_display: None,
        initial_plan_type: None,
        model: Some(resolved_model.clone()),
        startup_tooltip_override: None,
        status_line_invalid_items_warned: Arc::new(AtomicBool::new(false)),
        terminal_title_invalid_items_warned: Arc::new(AtomicBool::new(false)),
        session_telemetry,
    };

    let chat = ChatWidget::new_with_app_event(init);
    assert_eq!(chat.active_collaboration_mode_kind(), ModeKind::Default);
    assert_eq!(chat.current_model(), resolved_model);
}

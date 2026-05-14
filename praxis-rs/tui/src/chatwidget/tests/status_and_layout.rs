use super::*;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

fn test_team(team_id: &str, lead_thread_id: &str) -> praxis_app_server_protocol::Team {
    praxis_app_server_protocol::Team {
        id: team_id.to_string(),
        lead_thread_id: lead_thread_id.to_string(),
        name: "Test Team".to_string(),
        objective: None,
        execution_mode: praxis_app_server_protocol::TeamExecutionMode::ProcessFirst,
        resume_mode: praxis_app_server_protocol::TeamResumeMode::StrongResume,
        created_at: 1,
        updated_at: 1,
    }
}

fn test_team_task(
    team_id: &str,
    task_id: &str,
    title: &str,
    description: Option<&str>,
    status: praxis_app_server_protocol::TeamTaskStatus,
    created_at: i64,
    updated_at: i64,
) -> praxis_app_server_protocol::TeamTask {
    praxis_app_server_protocol::TeamTask {
        team_id: team_id.to_string(),
        task_id: task_id.to_string(),
        title: title.to_string(),
        description: description.map(str::to_string),
        status,
        assignee_teammate_id: None,
        created_at,
        updated_at,
        completed_at: None,
    }
}

fn test_teammate(
    team_id: &str,
    teammate_id: &str,
    name: &str,
    thread_id: &str,
) -> praxis_app_server_protocol::TeamTeammate {
    praxis_app_server_protocol::TeamTeammate {
        team_id: team_id.to_string(),
        teammate_id: teammate_id.to_string(),
        name: name.to_string(),
        role: None,
        status: praxis_app_server_protocol::TeamTeammateStatus::Active,
        thread_id: Some(thread_id.to_string()),
        last_error: None,
        created_at: 1,
        updated_at: 1,
    }
}

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
async fn context_budget_footer_stacks_above_team_task_footer() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.config.model_auto_compact_token_limit = Some(200_000);
    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: Some(950_000),
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let thread_id = ThreadId::new();
    let thread_id_text = thread_id.to_string();
    chat.thread_id = Some(thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &thread_id_text),
    });
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-1",
                "Audit diff",
                Some("Inspect the rendered patch"),
                praxis_app_server_protocol::TeamTaskStatus::InProgress,
                1,
                2,
            ),
        },
    );
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-2",
                "Apply patch",
                None,
                praxis_app_server_protocol::TeamTaskStatus::Pending,
                2,
                2,
            ),
        },
    );
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
        &[
            "Context: 127K / 200K (64%)".to_string(),
            "Up next: Apply patch".to_string(),
        ]
    );
}

#[tokio::test]
async fn clearing_token_info_restores_team_task_footer_copy() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.config.model_auto_compact_token_limit = Some(200_000);
    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: Some(950_000),
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let thread_id = ThreadId::new();
    let thread_id_text = thread_id.to_string();
    chat.thread_id = Some(thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &thread_id_text),
    });
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-1",
                "Audit diff",
                Some("Inspect the rendered patch"),
                praxis_app_server_protocol::TeamTaskStatus::InProgress,
                1,
                2,
            ),
        },
    );
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
        &[
            "Context: 127K / 200K (64%)".to_string(),
            "Run /status for the live breakdown".to_string(),
        ]
    );

    chat.handle_praxis_event(Event {
        id: "token-cleared".into(),
        msg: EventMsg::TokenCount(TokenCountEvent {
            info: None,
            rate_limits: None,
        }),
    });

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should remain visible");
    assert_eq!(
        status.footer_lines(),
        &["Run /status for the live breakdown".to_string()]
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
        frame_requester: FrameRequester::test_dummy(),
        app_event_tx: tx,
        initial_user_message: None,
        enhanced_keys_supported: false,
        has_chatgpt_account: false,
        model_catalog: test_model_catalog(&cfg),
        feedback: praxis_feedback::CodexFeedback::new(),
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

#[tokio::test]
async fn team_task_status_shows_status_tip_when_only_current_task_exists() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let thread_id = ThreadId::new();
    let thread_id_text = thread_id.to_string();
    chat.thread_id = Some(thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &thread_id_text),
    });
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-1",
                "Audit diff",
                Some("Inspect the rendered patch"),
                praxis_app_server_protocol::TeamTaskStatus::InProgress,
                1,
                2,
            ),
        },
    );

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), "Audit diff");
    assert_eq!(status.details(), Some("Inspect the rendered patch"));
    assert_eq!(
        status.footer_lines(),
        &["Run /status for the live breakdown".to_string()]
    );
}

#[tokio::test]
async fn team_task_status_prefers_up_next_footer_over_tip() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let thread_id = ThreadId::new();
    let thread_id_text = thread_id.to_string();
    chat.thread_id = Some(thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &thread_id_text),
    });
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-1",
                "Audit diff",
                Some("Inspect the rendered patch"),
                praxis_app_server_protocol::TeamTaskStatus::InProgress,
                1,
                2,
            ),
        },
    );
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-2",
                "Apply patch",
                None,
                praxis_app_server_protocol::TeamTaskStatus::Pending,
                2,
                2,
            ),
        },
    );

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), "Audit diff");
    assert_eq!(status.details(), Some("Inspect the rendered patch"));
    assert_eq!(status.footer_lines(), &["Up next: Apply patch".to_string()]);
}

#[tokio::test]
async fn team_task_keeps_status_visible_after_agent_turn_completes() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let thread_id = ThreadId::new();
    let thread_id_text = thread_id.to_string();
    chat.thread_id = Some(thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &thread_id_text),
    });
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-1",
                "Audit diff",
                Some("Inspect the rendered patch"),
                praxis_app_server_protocol::TeamTaskStatus::InProgress,
                1,
                2,
            ),
        },
    );

    chat.on_task_complete(/*last_agent_message*/ None, /*from_replay*/ true);

    assert!(chat.bottom_pane.status_indicator_visible());
    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should remain visible");
    assert_eq!(status.header(), "Audit diff");
    assert_eq!(status.details(), Some("Inspect the rendered patch"));
    assert_eq!(
        status.footer_lines(),
        &["Run /status for the live breakdown".to_string()]
    );
    assert!(!status.interrupt_hint_visible());
}

#[tokio::test]
async fn completed_team_task_hides_status_when_no_other_work_is_running() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    let thread_id = ThreadId::new();
    let thread_id_text = thread_id.to_string();
    chat.thread_id = Some(thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &thread_id_text),
    });
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-1",
                "Audit diff",
                Some("Inspect the rendered patch"),
                praxis_app_server_protocol::TeamTaskStatus::InProgress,
                1,
                2,
            ),
        },
    );

    assert!(chat.bottom_pane.status_indicator_visible());

    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-1",
                "Audit diff",
                Some("Inspect the rendered patch"),
                praxis_app_server_protocol::TeamTaskStatus::Completed,
                1,
                3,
            ),
        },
    );

    assert!(!chat.bottom_pane.status_indicator_visible());
}

#[tokio::test]
async fn lead_thread_team_task_prefixes_assignee_in_header_and_footer() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let lead_thread_id = ThreadId::new();
    let lead_thread_id_text = lead_thread_id.to_string();
    chat.thread_id = Some(lead_thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &lead_thread_id_text),
    });
    chat.on_team_teammate_updated_notification(
        praxis_app_server_protocol::TeamTeammateUpdatedNotification {
            team_id: "team-1".to_string(),
            teammate: test_teammate("team-1", "teammate-1", "alice", "worker-thread"),
            thread: None,
        },
    );

    let mut current_task = test_team_task(
        "team-1",
        "task-1",
        "Audit diff",
        Some("Inspect the rendered patch"),
        praxis_app_server_protocol::TeamTaskStatus::InProgress,
        1,
        2,
    );
    current_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: current_task,
        },
    );

    let mut next_task = test_team_task(
        "team-1",
        "task-2",
        "Apply patch",
        None,
        praxis_app_server_protocol::TeamTaskStatus::Pending,
        2,
        2,
    );
    next_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: next_task,
        },
    );

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), "alice: Audit diff");
    assert_eq!(status.details(), Some("Inspect the rendered patch"));
    assert_eq!(
        status.footer_lines(),
        &[
            "Test Team · 1 teammate".to_string(),
            "Up next: Apply patch (alice)".to_string(),
        ]
    );
}

#[tokio::test]
async fn teammate_thread_omits_self_assignee_prefix_in_header_and_footer() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let worker_thread_id = ThreadId::new();
    let worker_thread_id_text = worker_thread_id.to_string();
    chat.thread_id = Some(worker_thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", "lead-thread"),
    });
    chat.on_team_teammate_updated_notification(
        praxis_app_server_protocol::TeamTeammateUpdatedNotification {
            team_id: "team-1".to_string(),
            teammate: test_teammate("team-1", "teammate-1", "alice", &worker_thread_id_text),
            thread: None,
        },
    );

    let mut current_task = test_team_task(
        "team-1",
        "task-1",
        "Audit diff",
        Some("Inspect the rendered patch"),
        praxis_app_server_protocol::TeamTaskStatus::InProgress,
        1,
        2,
    );
    current_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: current_task,
        },
    );

    let mut next_task = test_team_task(
        "team-1",
        "task-2",
        "Apply patch",
        None,
        praxis_app_server_protocol::TeamTaskStatus::Pending,
        2,
        2,
    );
    next_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: next_task,
        },
    );

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), "Audit diff");
    assert_eq!(status.details(), Some("Inspect the rendered patch"));
    assert_eq!(status.footer_lines(), &["Up next: Apply patch".to_string()]);
}

#[tokio::test]
async fn lead_thread_team_task_summary_includes_extra_queue_counts() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let lead_thread_id = ThreadId::new();
    let lead_thread_id_text = lead_thread_id.to_string();
    chat.thread_id = Some(lead_thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &lead_thread_id_text),
    });
    chat.on_team_teammate_updated_notification(
        praxis_app_server_protocol::TeamTeammateUpdatedNotification {
            team_id: "team-1".to_string(),
            teammate: test_teammate("team-1", "teammate-1", "alice", "worker-thread"),
            thread: None,
        },
    );

    let mut current_task = test_team_task(
        "team-1",
        "task-1",
        "Audit diff",
        Some("Inspect the rendered patch"),
        praxis_app_server_protocol::TeamTaskStatus::InProgress,
        1,
        2,
    );
    current_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: current_task,
        },
    );

    let mut next_task = test_team_task(
        "team-1",
        "task-2",
        "Apply patch",
        None,
        praxis_app_server_protocol::TeamTaskStatus::Pending,
        2,
        2,
    );
    next_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: next_task,
        },
    );

    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: {
                let mut queued_task = test_team_task(
                    "team-1",
                    "task-3",
                    "Write regression test",
                    None,
                    praxis_app_server_protocol::TeamTaskStatus::Pending,
                    3,
                    3,
                );
                queued_task.assignee_teammate_id = Some("teammate-1".to_string());
                queued_task
            },
        },
    );

    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: test_team_task(
                "team-1",
                "task-4",
                "Wait on approval",
                None,
                praxis_app_server_protocol::TeamTaskStatus::Blocked,
                4,
                4,
            ),
        },
    );

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), "alice: Audit diff");
    assert_eq!(
        status.footer_lines(),
        &[
            "Test Team · 1 teammate · 1 more queued · 1 blocked".to_string(),
            "Up next: Apply patch (alice)".to_string(),
            "Queue: Write regression test (alice)".to_string(),
        ]
    );
}

#[tokio::test]
async fn lead_thread_queue_preview_includes_assignee_and_overflow_count() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_praxis_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    let lead_thread_id = ThreadId::new();
    let lead_thread_id_text = lead_thread_id.to_string();
    chat.thread_id = Some(lead_thread_id);

    chat.on_team_updated_notification(praxis_app_server_protocol::TeamUpdatedNotification {
        team: test_team("team-1", &lead_thread_id_text),
    });
    chat.on_team_teammate_updated_notification(
        praxis_app_server_protocol::TeamTeammateUpdatedNotification {
            team_id: "team-1".to_string(),
            teammate: test_teammate("team-1", "teammate-1", "alice", "worker-thread-a"),
            thread: None,
        },
    );
    chat.on_team_teammate_updated_notification(
        praxis_app_server_protocol::TeamTeammateUpdatedNotification {
            team_id: "team-1".to_string(),
            teammate: test_teammate("team-1", "teammate-2", "bob", "worker-thread-b"),
            thread: None,
        },
    );

    let mut current_task = test_team_task(
        "team-1",
        "task-1",
        "Audit diff",
        Some("Inspect the rendered patch"),
        praxis_app_server_protocol::TeamTaskStatus::InProgress,
        1,
        2,
    );
    current_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: current_task,
        },
    );

    let mut next_task = test_team_task(
        "team-1",
        "task-2",
        "Apply patch",
        None,
        praxis_app_server_protocol::TeamTaskStatus::Pending,
        2,
        2,
    );
    next_task.assignee_teammate_id = Some("teammate-1".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: next_task,
        },
    );

    let mut queued_bob = test_team_task(
        "team-1",
        "task-3",
        "Review logs",
        None,
        praxis_app_server_protocol::TeamTaskStatus::Pending,
        3,
        3,
    );
    queued_bob.assignee_teammate_id = Some("teammate-2".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: queued_bob,
        },
    );

    let mut queued_docs = test_team_task(
        "team-1",
        "task-4",
        "Update docs",
        None,
        praxis_app_server_protocol::TeamTaskStatus::Pending,
        4,
        4,
    );
    queued_docs.assignee_teammate_id = Some("teammate-2".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: queued_docs,
        },
    );

    let mut overflow = test_team_task(
        "team-1",
        "task-5",
        "Run snapshots",
        None,
        praxis_app_server_protocol::TeamTaskStatus::Pending,
        5,
        5,
    );
    overflow.assignee_teammate_id = Some("teammate-2".to_string());
    chat.on_team_task_updated_notification(
        praxis_app_server_protocol::TeamTaskUpdatedNotification {
            team_id: "team-1".to_string(),
            task: overflow,
        },
    );

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), "alice: Audit diff");
    assert_eq!(
        status.footer_lines(),
        &[
            "Test Team · 2 teammates · 3 more queued".to_string(),
            "Up next: Apply patch (alice)".to_string(),
            "Queue: Review logs (bob), Update docs (bob) +1 more".to_string(),
        ]
    );
}

#[derive(Debug)]
struct CountingHistoryCell {
    display_calls: Arc<AtomicUsize>,
    mouse_calls: Arc<AtomicUsize>,
    thread_id: ThreadId,
}

impl HistoryCell for CountingHistoryCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        self.display_calls.fetch_add(1, Ordering::SeqCst);
        vec![Line::from("cached active cell")]
    }

    fn mouse_targets(&self, _width: u16) -> Vec<history_cell::HistoryCellMouseTarget> {
        self.mouse_calls.fetch_add(1, Ordering::SeqCst);
        vec![history_cell::HistoryCellMouseTarget {
            row_start: 0,
            row_end: 0,
            action: history_cell::HistoryCellMouseAction::ResumeRecentThread {
                thread_id: self.thread_id,
                thread_name: "Cached".to_string(),
            },
        }]
    }
}

#[tokio::test]
async fn active_cell_render_cache_reuses_computed_lines_within_same_revision() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let display_calls = Arc::new(AtomicUsize::new(0));
    let mouse_calls = Arc::new(AtomicUsize::new(0));

    chat.active_cell = Some(Box::new(CountingHistoryCell {
        display_calls: display_calls.clone(),
        mouse_calls: mouse_calls.clone(),
        thread_id: ThreadId::new(),
    }));
    chat.bump_active_cell_revision();

    let width: u16 = 80;
    let height = chat.desired_height(width);
    assert_eq!(display_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mouse_calls.load(Ordering::SeqCst), 1);

    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    chat.render(area, &mut buf);

    assert_eq!(display_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mouse_calls.load(Ordering::SeqCst), 1);

    let action =
        chat.active_cell_mouse_action(area, area.x, area.y.saturating_add(CHAT_SECTION_GAP_ROWS));
    assert!(matches!(
        action,
        Some(history_cell::HistoryCellMouseAction::ResumeRecentThread { .. })
    ));
    assert_eq!(display_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mouse_calls.load(Ordering::SeqCst), 1);

    chat.bump_active_cell_revision();
    let _ = chat.desired_height(width);
    assert_eq!(display_calls.load(Ordering::SeqCst), 2);
    assert_eq!(mouse_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn rate_limit_warnings_emit_thresholds() {
    let mut state = RateLimitWarningState::default();
    let mut warnings: Vec<String> = Vec::new();

    warnings.extend(state.take_warnings(Some(10.0), Some(10079), Some(55.0), Some(299)));
    warnings.extend(state.take_warnings(Some(55.0), Some(10081), Some(10.0), Some(299)));
    warnings.extend(state.take_warnings(Some(10.0), Some(10081), Some(80.0), Some(299)));
    warnings.extend(state.take_warnings(Some(80.0), Some(10081), Some(10.0), Some(299)));
    warnings.extend(state.take_warnings(Some(10.0), Some(10081), Some(95.0), Some(299)));
    warnings.extend(state.take_warnings(Some(95.0), Some(10079), Some(10.0), Some(299)));

    assert_eq!(
        warnings,
        vec![
            String::from(
                "Heads up, you have less than 25% of your 5h limit left. Run /status for a breakdown."
            ),
            String::from(
                "Heads up, you have less than 25% of your weekly limit left. Run /status for a breakdown.",
            ),
            String::from(
                "Heads up, you have less than 5% of your 5h limit left. Run /status for a breakdown."
            ),
            String::from(
                "Heads up, you have less than 5% of your weekly limit left. Run /status for a breakdown.",
            ),
        ],
        "expected one warning per limit for the highest crossed threshold"
    );
}

#[tokio::test]
async fn test_rate_limit_warnings_monthly() {
    let mut state = RateLimitWarningState::default();
    let mut warnings: Vec<String> = Vec::new();

    warnings.extend(state.take_warnings(
        Some(75.0),
        Some(43199),
        /*primary_used_percent*/ None,
        /*primary_window_minutes*/ None,
    ));
    assert_eq!(
        warnings,
        vec![String::from(
            "Heads up, you have less than 25% of your monthly limit left. Run /status for a breakdown.",
        ),],
        "expected one warning per limit for the highest crossed threshold"
    );
}

#[tokio::test]
async fn rate_limit_snapshot_keeps_prior_credits_when_missing_from_headers() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: None,
        secondary: None,
        credits: Some(CreditsSnapshot {
            has_credits: true,
            unlimited: false,
            balance: Some("17.5".to_string()),
        }),
        plan_type: None,
    }));
    let initial_balance = chat
        .rate_limit_snapshots_by_limit_id
        .get("codex")
        .and_then(|snapshot| snapshot.credits.as_ref())
        .and_then(|credits| credits.balance.as_deref());
    assert_eq!(initial_balance, Some("17.5"));

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: Some(RateLimitWindow {
            used_percent: 80.0,
            window_minutes: Some(60),
            resets_at: Some(123),
        }),
        secondary: None,
        credits: None,
        plan_type: None,
    }));

    let display = chat
        .rate_limit_snapshots_by_limit_id
        .get("codex")
        .expect("rate limits should be cached");
    let credits = display
        .credits
        .as_ref()
        .expect("credits should persist when headers omit them");

    assert_eq!(credits.balance.as_deref(), Some("17.5"));
    assert!(!credits.unlimited);
    assert_eq!(
        display.primary.as_ref().map(|window| window.used_percent),
        Some(80.0)
    );
}

#[tokio::test]
async fn rate_limit_snapshot_updates_and_retains_plan_type() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: Some(RateLimitWindow {
            used_percent: 10.0,
            window_minutes: Some(60),
            resets_at: None,
        }),
        secondary: Some(RateLimitWindow {
            used_percent: 5.0,
            window_minutes: Some(300),
            resets_at: None,
        }),
        credits: None,
        plan_type: Some(PlanType::Plus),
    }));
    assert_eq!(chat.plan_type, Some(PlanType::Plus));

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: Some(RateLimitWindow {
            used_percent: 25.0,
            window_minutes: Some(30),
            resets_at: Some(123),
        }),
        secondary: Some(RateLimitWindow {
            used_percent: 15.0,
            window_minutes: Some(300),
            resets_at: Some(234),
        }),
        credits: None,
        plan_type: Some(PlanType::Pro),
    }));
    assert_eq!(chat.plan_type, Some(PlanType::Pro));

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: Some(RateLimitWindow {
            used_percent: 30.0,
            window_minutes: Some(60),
            resets_at: Some(456),
        }),
        secondary: Some(RateLimitWindow {
            used_percent: 18.0,
            window_minutes: Some(300),
            resets_at: Some(567),
        }),
        credits: None,
        plan_type: None,
    }));
    assert_eq!(chat.plan_type, Some(PlanType::Pro));
}

#[tokio::test]
async fn rate_limit_snapshots_keep_separate_entries_per_limit_id() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: Some("codex".to_string()),
        limit_name: Some("codex".to_string()),
        primary: Some(RateLimitWindow {
            used_percent: 20.0,
            window_minutes: Some(300),
            resets_at: Some(100),
        }),
        secondary: None,
        credits: Some(CreditsSnapshot {
            has_credits: true,
            unlimited: false,
            balance: Some("5.00".to_string()),
        }),
        plan_type: Some(PlanType::Pro),
    }));

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: Some("praxis_other".to_string()),
        limit_name: Some("praxis_other".to_string()),
        primary: Some(RateLimitWindow {
            used_percent: 90.0,
            window_minutes: Some(60),
            resets_at: Some(200),
        }),
        secondary: None,
        credits: None,
        plan_type: Some(PlanType::Pro),
    }));

    let codex = chat
        .rate_limit_snapshots_by_limit_id
        .get("codex")
        .expect("codex snapshot should exist");
    let other = chat
        .rate_limit_snapshots_by_limit_id
        .get("praxis_other")
        .expect("praxis_other snapshot should exist");

    assert_eq!(codex.primary.as_ref().map(|w| w.used_percent), Some(20.0));
    assert_eq!(
        codex
            .credits
            .as_ref()
            .and_then(|credits| credits.balance.as_deref()),
        Some("5.00")
    );
    assert_eq!(other.primary.as_ref().map(|w| w.used_percent), Some(90.0));
    assert!(other.credits.is_none());
}

#[tokio::test]
async fn rate_limit_switch_prompt_skips_when_on_lower_cost_model() {
    let (mut chat, _, _) = make_chatwidget_manual(Some(NUDGE_MODEL_SLUG)).await;
    chat.has_chatgpt_account = true;

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 95.0)));

    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Idle
    ));
}

#[tokio::test]
async fn rate_limit_switch_prompt_skips_non_praxis_limit() {
    let (mut chat, _, _) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.has_chatgpt_account = true;

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        limit_id: Some("praxis_other".to_string()),
        limit_name: Some("praxis_other".to_string()),
        primary: Some(RateLimitWindow {
            used_percent: 95.0,
            window_minutes: Some(60),
            resets_at: None,
        }),
        secondary: None,
        credits: None,
        plan_type: None,
    }));

    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Idle
    ));
}

#[tokio::test]
async fn rate_limit_switch_prompt_shows_once_per_session() {
    let (mut chat, _, _) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.has_chatgpt_account = true;

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 90.0)));
    assert!(
        chat.rate_limit_warnings.primary_index >= 1,
        "warnings not emitted"
    );
    chat.maybe_show_pending_rate_limit_prompt();
    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Shown
    ));

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 95.0)));
    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Shown
    ));
}

#[tokio::test]
async fn rate_limit_switch_prompt_respects_hidden_notice() {
    let (mut chat, _, _) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.has_chatgpt_account = true;
    chat.config.notices.hide_rate_limit_model_nudge = Some(true);

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 95.0)));

    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Idle
    ));
}

#[tokio::test]
async fn rate_limit_switch_prompt_defers_until_task_complete() {
    let (mut chat, _, _) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.has_chatgpt_account = true;

    chat.bottom_pane.set_task_running(/*running*/ true);
    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 90.0)));
    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Pending
    ));

    chat.bottom_pane.set_task_running(/*running*/ false);
    chat.maybe_show_pending_rate_limit_prompt();
    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Shown
    ));
}

#[tokio::test]
async fn rate_limit_switch_prompt_popup_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.has_chatgpt_account = true;

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 92.0)));
    chat.maybe_show_pending_rate_limit_prompt();

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("rate_limit_switch_prompt_popup", popup);
}

#[tokio::test]
async fn streaming_final_answer_keeps_task_running_state() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());

    chat.on_task_started();
    chat.on_agent_message_delta("Final answer line\n".to_string());
    chat.on_commit_tick();
    drain_insert_history(&mut rx);

    assert!(chat.bottom_pane.is_task_running());
    assert!(!chat.bottom_pane.status_indicator_visible());

    chat.bottom_pane
        .set_composer_text("queued submission".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(chat.queued_user_messages.len(), 1);
    assert_eq!(
        chat.queued_user_messages.front().unwrap().text,
        "queued submission"
    );
    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));

    chat.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    match op_rx.try_recv() {
        Ok(Op::Interrupt) => {}
        other => panic!("expected Op::Interrupt, got {other:?}"),
    }
    assert!(!chat.bottom_pane.quit_shortcut_hint_visible());
}

#[tokio::test]
async fn idle_commit_ticks_do_not_restore_status_without_commentary_completion() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_task_started();
    assert_eq!(chat.bottom_pane.status_indicator_visible(), true);

    chat.on_agent_message_delta("Final answer line\n".to_string());
    chat.on_commit_tick();
    drain_insert_history(&mut rx);

    assert_eq!(chat.bottom_pane.status_indicator_visible(), false);
    assert_eq!(chat.bottom_pane.is_task_running(), true);

    // A second idle tick should not toggle the row back on and cause jitter.
    chat.on_commit_tick();
    assert_eq!(chat.bottom_pane.status_indicator_visible(), false);
}

#[tokio::test]
async fn commentary_completion_restores_status_indicator_before_exec_begin() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_task_started();
    assert_eq!(chat.bottom_pane.status_indicator_visible(), true);

    chat.on_agent_message_delta("Preamble line\n".to_string());
    chat.on_commit_tick();
    drain_insert_history(&mut rx);

    assert_eq!(chat.bottom_pane.status_indicator_visible(), false);

    complete_assistant_message(
        &mut chat,
        "msg-commentary",
        "Preamble line\n",
        Some(MessagePhase::Commentary),
    );

    assert_eq!(chat.bottom_pane.status_indicator_visible(), true);
    assert_eq!(chat.bottom_pane.is_task_running(), true);

    begin_exec(&mut chat, "call-1", "echo hi");
    assert_eq!(chat.bottom_pane.status_indicator_visible(), true);
}

#[tokio::test]
async fn fast_status_indicator_requires_chatgpt_auth() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    chat.set_service_tier(Some(ServiceTier::Fast));

    assert!(!chat.should_show_fast_status(chat.current_model(), chat.current_service_tier(),));

    set_chatgpt_auth(&mut chat);

    assert!(chat.should_show_fast_status(chat.current_model(), chat.current_service_tier(),));
}

#[tokio::test]
async fn fast_status_indicator_is_hidden_for_non_gpt54_model() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-codex")).await;
    chat.set_service_tier(Some(ServiceTier::Fast));
    set_chatgpt_auth(&mut chat);

    assert!(!chat.should_show_fast_status(chat.current_model(), chat.current_service_tier(),));
}

#[tokio::test]
async fn fast_status_indicator_is_hidden_when_fast_mode_is_off() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    set_chatgpt_auth(&mut chat);

    assert!(!chat.should_show_fast_status(chat.current_model(), chat.current_service_tier(),));
}

// Snapshot test: ChatWidget at very small heights (idle)
// Ensures overall layout behaves when terminal height is extremely constrained.
#[tokio::test]
async fn ui_snapshots_small_heights_idle() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let (chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    for h in [1u16, 2, 3] {
        let name = format!("chat_small_idle_h{h}");
        let mut terminal = Terminal::new(TestBackend::new(40, h)).expect("create terminal");
        terminal
            .draw(|f| chat.render(f.area(), f.buffer_mut()))
            .expect("draw chat idle");
        assert_chatwidget_snapshot!(name, normalized_backend_snapshot(terminal.backend()));
    }
}

// Snapshot test: ChatWidget at very small heights (task running)
// Validates how status + composer are presented within tight space.
#[tokio::test]
async fn ui_snapshots_small_heights_task_running() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    // Activate status line
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Thinking**".into(),
        }),
    });
    for h in [1u16, 2, 3] {
        let name = format!("chat_small_running_h{h}");
        let mut terminal = Terminal::new(TestBackend::new(40, h)).expect("create terminal");
        terminal
            .draw(|f| chat.render(f.area(), f.buffer_mut()))
            .expect("draw chat running");
        assert_chatwidget_snapshot!(name, normalized_backend_snapshot(terminal.backend()));
    }
}

// Snapshot test: status widget + approval modal active together
// The modal takes precedence visually; this captures the layout with a running
// task (status indicator active) while an approval request is shown.
#[tokio::test]
async fn status_widget_and_approval_modal_snapshot() {
    use praxis_protocol::protocol::ExecApprovalRequestEvent;

    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    // Begin a running task so the status indicator would be active.
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    // Provide a deterministic header for the status line.
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Analyzing**".into(),
        }),
    });

    // Now show an approval modal (e.g. exec approval).
    let ev = ExecApprovalRequestEvent {
        call_id: "call-approve-exec".into(),
        approval_id: Some("call-approve-exec".into()),
        turn_id: "turn-approve-exec".into(),
        command: vec!["echo".into(), "hello world".into()],
        cwd: PathBuf::from("/tmp"),
        reason: Some(
            "this is a test reason such as one that would be produced by the model".into(),
        ),
        network_approval_context: None,
        proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(vec![
            "echo".into(),
            "hello world".into(),
        ])),
        proposed_network_policy_amendments: None,
        additional_permissions: None,
        available_decisions: None,
        parsed_cmd: vec![],
    };
    chat.handle_praxis_event(Event {
        id: "sub-approve-exec".into(),
        msg: EventMsg::ExecApprovalRequest(ev),
    });

    // Render at the widget's desired height and snapshot.
    let width: u16 = 100;
    let height = chat.desired_height(width);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("create terminal");
    terminal.set_viewport_area(Rect::new(0, 0, width, height));
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw status + approval modal");
    assert_chatwidget_snapshot!(
        "status_widget_and_approval_modal",
        normalized_backend_snapshot(terminal.backend())
    );
}

// Snapshot test: status widget active (StatusIndicatorView)
// Ensures the VT100 rendering of the status indicator is stable when active.
#[tokio::test]
async fn status_widget_active_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    // Activate the status indicator by simulating a task start.
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    // Provide a deterministic header via a bold reasoning chunk.
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Analyzing**".into(),
        }),
    });
    // Render and snapshot.
    let height = chat.desired_height(/*width*/ 80);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height))
        .expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw status widget");
    assert_chatwidget_snapshot!(
        "status_widget_active",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn stream_error_updates_status_indicator() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.bottom_pane.set_task_running(/*running*/ true);
    let msg = "Reconnecting... 2/5";
    let details = "Idle timeout waiting for SSE";
    chat.handle_praxis_event(Event {
        id: "sub-1".into(),
        msg: EventMsg::StreamError(StreamErrorEvent {
            message: msg.to_string(),
            praxis_error_info: Some(CodexErrorInfo::Other),
            additional_details: Some(details.to_string()),
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert!(
        cells.is_empty(),
        "expected no history cell for StreamError event"
    );
    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), msg);
    assert_eq!(status.details(), Some(details));
}

#[tokio::test]
async fn stream_error_restores_hidden_status_indicator() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.on_task_started();
    chat.on_agent_message_delta("Preamble line\n".to_string());
    chat.on_commit_tick();
    drain_insert_history(&mut rx);
    assert!(!chat.bottom_pane.status_indicator_visible());

    let msg = "Reconnecting... 2/5";
    let details = "Idle timeout waiting for SSE";
    chat.handle_praxis_event(Event {
        id: "sub-1".into(),
        msg: EventMsg::StreamError(StreamErrorEvent {
            message: msg.to_string(),
            praxis_error_info: Some(CodexErrorInfo::Other),
            additional_details: Some(details.to_string()),
        }),
    });

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be visible");
    assert_eq!(status.header(), msg);
    assert_eq!(status.details(), Some(details));
}

#[tokio::test]
async fn warning_event_adds_warning_history_cell() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.handle_praxis_event(Event {
        id: "sub-1".into(),
        msg: EventMsg::Warning(WarningEvent {
            message: "test warning message".to_string(),
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one warning history cell");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("test warning message"),
        "warning cell missing content: {rendered}"
    );
}

#[tokio::test]
async fn status_line_invalid_items_warn_once() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.config.tui_status_line = Some(vec![
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
    chat.config.tui_status_line = Some(vec!["model_name".to_string()]);

    chat.refresh_status_line();

    assert_eq!(chat.status_line_branch, None);
    assert!(!chat.status_line_branch_pending);
    assert!(!chat.status_line_branch_lookup_complete);
}

#[tokio::test]
async fn status_line_branch_refreshes_after_turn_complete() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.config.tui_status_line = Some(vec!["git-branch".to_string()]);
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
    chat.config.tui_status_line = Some(vec!["git-branch".to_string()]);
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
    chat.config.tui_status_line = Some(vec!["fast-mode".to_string()]);

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
    chat.config.tui_status_line = Some(vec!["model-with-reasoning".to_string()]);
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
    chat.config.tui_status_line = Some(vec!["fast-mode".to_string()]);
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
    chat.config.tui_status_line = Some(vec![
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
    chat.config.tui_terminal_title = Some(vec!["model".to_string()]);
    chat.refresh_terminal_title();

    assert_eq!(chat.last_terminal_title, Some("gpt-5.4".to_string()));

    chat.set_model("gpt-5.3-codex");

    assert_eq!(chat.last_terminal_title, Some("gpt-5.3-codex".to_string()));
}

#[tokio::test]
async fn terminal_title_defaults_to_project_and_thread_name() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.config.tui_terminal_title = None;
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
    chat.config.tui_terminal_title = None;
    chat.config.cwd = test_project_path().abs();
    chat.thread_name = None;

    chat.refresh_terminal_title();

    assert_eq!(chat.last_terminal_title, Some("project".to_string()));
}

#[tokio::test]
async fn terminal_title_default_prefers_current_directory_name() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.config.tui_terminal_title = None;
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
    chat.config.tui_status_line = Some(vec!["model-with-reasoning".to_string()]);
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
    chat.config.tui_status_line = Some(vec!["model-with-reasoning".to_string()]);
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
    chat.config.tui_status_line = Some(vec![
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

#[tokio::test]
async fn runtime_metrics_websocket_timing_logs_and_final_separator_sums_totals() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::RuntimeMetrics, /*enabled*/ true);

    chat.on_task_started();
    chat.apply_runtime_metrics_delta(RuntimeMetricsSummary {
        responses_api_engine_iapi_ttft_ms: 120,
        responses_api_engine_service_tbt_ms: 50,
        ..RuntimeMetricsSummary::default()
    });

    let first_log = drain_insert_history(&mut rx)
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .find(|line| line.contains("WebSocket timing:"))
        .expect("expected websocket timing log");
    assert!(first_log.contains("TTFT: 120ms (iapi)"));
    assert!(first_log.contains("TBT: 50ms (service)"));

    chat.apply_runtime_metrics_delta(RuntimeMetricsSummary {
        responses_api_engine_iapi_ttft_ms: 80,
        ..RuntimeMetricsSummary::default()
    });

    let second_log = drain_insert_history(&mut rx)
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .find(|line| line.contains("WebSocket timing:"))
        .expect("expected websocket timing log");
    assert!(second_log.contains("TTFT: 80ms (iapi)"));

    chat.on_task_complete(/*last_agent_message*/ None, /*from_replay*/ false);
    let mut final_separator = None;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = event {
            final_separator = Some(lines_to_single_string(&cell.display_lines(/*width*/ 300)));
        }
    }
    let final_separator = final_separator.expect("expected final separator with runtime metrics");
    assert!(final_separator.contains("TTFT: 80ms (iapi)"));
    assert!(final_separator.contains("TBT: 50ms (service)"));
}

#[tokio::test]
async fn multiple_agent_messages_in_single_turn_emit_multiple_headers() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Begin turn
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    // First finalized assistant message
    complete_assistant_message(&mut chat, "msg-first", "First message", /*phase*/ None);

    // Second finalized assistant message in the same turn
    complete_assistant_message(
        &mut chat,
        "msg-second",
        "Second message",
        /*phase*/ None,
    );

    // End turn
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
        }),
    });

    let cells = drain_insert_history(&mut rx);
    let combined: String = cells
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect();
    assert!(
        combined.contains("First message"),
        "missing first message: {combined}"
    );
    assert!(
        combined.contains("Second message"),
        "missing second message: {combined}"
    );
    let first_idx = combined.find("First message").unwrap();
    let second_idx = combined.find("Second message").unwrap();
    assert!(first_idx < second_idx, "messages out of order: {combined}");
}

#[tokio::test]
async fn final_reasoning_then_message_without_deltas_are_rendered() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // No deltas; only final reasoning followed by final message.
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoning(AgentReasoningEvent {
            text: "I will first analyze the request.".into(),
        }),
    });
    complete_assistant_message(
        &mut chat,
        "msg-result",
        "Here is the result.",
        /*phase*/ None,
    );

    // Drain history and snapshot the combined visible content.
    let cells = drain_insert_history(&mut rx);
    let combined = cells
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();
    assert_chatwidget_snapshot!(
        "final_reasoning_then_message_without_deltas_are_rendered",
        combined
    );
}

#[tokio::test]
async fn deltas_then_same_final_message_are_rendered_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Stream some reasoning deltas first.
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "I will ".into(),
        }),
    });
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "first analyze the ".into(),
        }),
    });
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "request.".into(),
        }),
    });
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoning(AgentReasoningEvent {
            text: "request.".into(),
        }),
    });

    // Then stream answer deltas, followed by the exact same final message.
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Here is the ".into(),
        }),
    });
    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "result.".into(),
        }),
    });

    chat.handle_praxis_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Here is the result.".into(),
            phase: None,
            memory_citation: None,
        }),
    });

    // Snapshot the combined visible content to ensure we render as expected
    // when deltas are followed by the identical final message.
    let cells = drain_insert_history(&mut rx);
    let combined = cells
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();
    assert_chatwidget_snapshot!(
        "deltas_then_same_final_message_are_rendered_snapshot",
        combined
    );
}

#[tokio::test]
async fn user_prompt_submit_app_server_hook_notifications_render_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_server_notification(
        ServerNotification::HookStarted(AppServerHookStartedNotification {
            thread_id: ThreadId::new().to_string(),
            turn_id: Some("turn-1".to_string()),
            run: AppServerHookRunSummary {
                id: "user-prompt-submit:0:/tmp/hooks.json".to_string(),
                event_name: AppServerHookEventName::UserPromptSubmit,
                handler_type: AppServerHookHandlerType::Command,
                execution_mode: AppServerHookExecutionMode::Sync,
                scope: AppServerHookScope::Turn,
                source_path: PathBuf::from("/tmp/hooks.json"),
                display_order: 0,
                status: AppServerHookRunStatus::Running,
                status_message: Some("checking go-workflow input policy".to_string()),
                started_at: 1,
                completed_at: None,
                duration_ms: None,
                entries: Vec::new(),
            },
        }),
        /*replay_kind*/ None,
    );
    chat.handle_server_notification(
        ServerNotification::HookCompleted(AppServerHookCompletedNotification {
            thread_id: ThreadId::new().to_string(),
            turn_id: Some("turn-1".to_string()),
            run: AppServerHookRunSummary {
                id: "user-prompt-submit:0:/tmp/hooks.json".to_string(),
                event_name: AppServerHookEventName::UserPromptSubmit,
                handler_type: AppServerHookHandlerType::Command,
                execution_mode: AppServerHookExecutionMode::Sync,
                scope: AppServerHookScope::Turn,
                source_path: PathBuf::from("/tmp/hooks.json"),
                display_order: 0,
                status: AppServerHookRunStatus::Stopped,
                status_message: Some("checking go-workflow input policy".to_string()),
                started_at: 1,
                completed_at: Some(11),
                duration_ms: Some(10),
                entries: vec![
                    AppServerHookOutputEntry {
                        kind: AppServerHookOutputEntryKind::Warning,
                        text: "go-workflow must start from PlanMode".to_string(),
                    },
                    AppServerHookOutputEntry {
                        kind: AppServerHookOutputEntryKind::Stop,
                        text: "prompt blocked".to_string(),
                    },
                ],
            },
        }),
        /*replay_kind*/ None,
    );

    let cells = drain_insert_history(&mut rx);
    let combined = cells
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();
    assert_chatwidget_snapshot!(
        "user_prompt_submit_app_server_hook_notifications_render_snapshot",
        combined
    );
}

#[tokio::test]
async fn pre_tool_use_hook_events_render_snapshot() {
    assert_hook_events_snapshot(
        praxis_protocol::protocol::HookEventName::PreToolUse,
        "pre-tool-use:0:/tmp/hooks.json",
        "warming the shell",
        "pre_tool_use_hook_events_render_snapshot",
    )
    .await;
}

#[tokio::test]
async fn post_tool_use_hook_events_render_snapshot() {
    assert_hook_events_snapshot(
        praxis_protocol::protocol::HookEventName::PostToolUse,
        "post-tool-use:0:/tmp/hooks.json",
        "warming the shell",
        "post_tool_use_hook_events_render_snapshot",
    )
    .await;
}

#[tokio::test]
async fn session_start_hook_events_render_snapshot() {
    assert_hook_events_snapshot(
        praxis_protocol::protocol::HookEventName::SessionStart,
        "session-start:0:/tmp/hooks.json",
        "warming the shell",
        "session_start_hook_events_render_snapshot",
    )
    .await;
}

// Combined visual snapshot using vt100 for history + direct buffer overlay for UI.
// This renders the final visual as seen in a terminal: history above, then a blank line,
// then the exec block, another blank line, the status line, a blank line, and the composer.
#[tokio::test]
async fn chatwidget_exec_and_status_layout_vt100_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    complete_assistant_message(
        &mut chat,
        "msg-search",
        "I’m going to search the repo for where “Change Approved” is rendered to update that view.",
        /*phase*/ None,
    );

    let command = vec!["bash".into(), "-lc".into(), "rg \"Change Approved\"".into()];
    let parsed_cmd = vec![
        ParsedCommand::Search {
            query: Some("Change Approved".into()),
            path: None,
            cmd: "rg \"Change Approved\"".into(),
        },
        ParsedCommand::Read {
            name: "diff_render.rs".into(),
            cmd: "cat diff_render.rs".into(),
            path: "diff_render.rs".into(),
        },
    ];
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    chat.handle_praxis_event(Event {
        id: "c1".into(),
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: "c1".into(),
            process_id: None,
            turn_id: "turn-1".into(),
            command: command.clone(),
            cwd: cwd.clone(),
            parsed_cmd: parsed_cmd.clone(),
            source: ExecCommandSource::Agent,
            interaction_input: None,
        }),
    });
    chat.handle_praxis_event(Event {
        id: "c1".into(),
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: "c1".into(),
            process_id: None,
            turn_id: "turn-1".into(),
            command,
            cwd,
            parsed_cmd,
            source: ExecCommandSource::Agent,
            interaction_input: None,
            stdout: String::new(),
            stderr: String::new(),
            aggregated_output: String::new(),
            exit_code: 0,
            duration: std::time::Duration::from_millis(16000),
            formatted_output: String::new(),
            status: CoreExecCommandStatus::Completed,
        }),
    });
    chat.handle_praxis_event(Event {
        id: "t1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    chat.handle_praxis_event(Event {
        id: "t1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Investigating rendering code**".into(),
        }),
    });
    chat.bottom_pane.set_composer_text(
        "Summarize recent commits".to_string(),
        Vec::new(),
        Vec::new(),
    );

    let width: u16 = 80;
    let ui_height: u16 = chat.desired_height(width);
    let vt_height: u16 = 40;
    let viewport = Rect::new(0, vt_height - ui_height - 1, width, ui_height);

    let backend = VT100Backend::new(width, vt_height);
    let mut term = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    term.set_viewport_area(viewport);

    for lines in drain_insert_history(&mut rx) {
        crate::insert_history::insert_history_lines(&mut term, lines)
            .expect("Failed to insert history lines in test");
    }

    term.draw(|f| {
        chat.render(f.area(), f.buffer_mut());
    })
    .unwrap();

    assert_chatwidget_snapshot!(
        "chatwidget_exec_and_status_layout_vt100_snapshot",
        normalize_snapshot_paths(term.backend().vt100().screen().contents())
    );
}

// E2E vt100 snapshot for complex markdown with indented and nested fenced code blocks
#[tokio::test]
async fn chatwidget_markdown_code_blocks_vt100_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Simulate a final agent message via streaming deltas instead of a single message

    chat.handle_praxis_event(Event {
        id: "t1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    // Build a vt100 visual from the history insertions only (no UI overlay)
    let width: u16 = 80;
    let height: u16 = 50;
    let backend = VT100Backend::new(width, height);
    let mut term = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    // Place viewport at the last line so that history lines insert above it
    term.set_viewport_area(Rect::new(0, height - 1, width, 1));

    // Simulate streaming via AgentMessageDelta in 2-character chunks (no final AgentMessage).
    let source: &str = r#"

    -- Indented code block (4 spaces)
    SELECT *
    FROM "users"
    WHERE "email" LIKE '%@example.com';

````markdown
```sh
printf 'fenced within fenced\n'
```
````

```jsonc
{
  // comment allowed in jsonc
  "path": "C:\\Program Files\\App",
  "regex": "^foo.*(bar)?$"
}
```
"#;

    let mut it = source.chars();
    loop {
        let mut delta = String::new();
        match it.next() {
            Some(c) => delta.push(c),
            None => break,
        }
        if let Some(c2) = it.next() {
            delta.push(c2);
        }

        chat.handle_praxis_event(Event {
            id: "t1".into(),
            msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }),
        });
        // Drive commit ticks and drain emitted history lines into the vt100 buffer.
        loop {
            chat.on_commit_tick();
            let mut inserted_any = false;
            while let Ok(app_ev) = rx.try_recv() {
                if let AppEvent::InsertHistoryCell(cell) = app_ev {
                    let lines = cell.display_lines(width);
                    crate::insert_history::insert_history_lines(&mut term, lines)
                        .expect("Failed to insert history lines in test");
                    inserted_any = true;
                }
            }
            if !inserted_any {
                break;
            }
        }
    }

    // Finalize the stream without sending a final AgentMessage, to flush any tail.
    chat.handle_praxis_event(Event {
        id: "t1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
        }),
    });
    for lines in drain_insert_history(&mut rx) {
        crate::insert_history::insert_history_lines(&mut term, lines)
            .expect("Failed to insert history lines in test");
    }

    assert_chatwidget_snapshot!(
        "chatwidget_markdown_code_blocks_vt100_snapshot",
        normalize_snapshot_paths(term.backend().vt100().screen().contents())
    );
}

#[tokio::test]
async fn chatwidget_tall() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.handle_praxis_event(Event {
        id: "t1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    for i in 0..30 {
        chat.queue_user_message(format!("Hello, world! {i}").into());
    }
    let width: u16 = 80;
    let height: u16 = 24;
    let backend = VT100Backend::new(width, height);
    let mut term = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    let desired_height = chat.desired_height(width).min(height);
    term.set_viewport_area(Rect::new(0, height - desired_height, width, desired_height));
    term.draw(|f| {
        chat.render(f.area(), f.buffer_mut());
    })
    .unwrap();
    assert_chatwidget_snapshot!(
        "chatwidget_tall",
        normalize_snapshot_paths(term.backend().vt100().screen().contents())
    );
}

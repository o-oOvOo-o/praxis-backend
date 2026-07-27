use super::*;

#[test]
fn thread_event_store_tracks_active_turn_lifecycle() {
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    assert_eq!(store.active_turn_id(), None);

    let thread_id = ThreadId::new();
    store.push_notification(turn_started_notification(thread_id, "turn-1"));
    assert_eq!(store.active_turn_id(), Some("turn-1"));

    store.push_notification(turn_completed_notification(
        thread_id,
        "turn-2",
        TurnStatus::Completed,
    ));
    assert_eq!(store.active_turn_id(), Some("turn-1"));

    store.push_notification(turn_completed_notification(
        thread_id,
        "turn-1",
        TurnStatus::Interrupted,
    ));
    assert_eq!(store.active_turn_id(), None);
}

#[test]
fn thread_event_store_restores_active_turn_from_snapshot_turns() {
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    let turns = vec![
        test_turn("turn-1", TurnStatus::Completed, Vec::new()),
        test_turn("turn-2", TurnStatus::InProgress, Vec::new()),
    ];

    let store =
        ThreadEventStore::new_with_session(/*capacity*/ 8, session.clone(), turns.clone());
    assert_eq!(store.active_turn_id(), Some("turn-2"));

    let mut refreshed_store = ThreadEventStore::new(/*capacity*/ 8);
    refreshed_store.set_session(session, turns);
    assert_eq!(refreshed_store.active_turn_id(), Some("turn-2"));
}

#[test]
fn thread_event_store_clear_active_turn_id_resets_cached_turn() {
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    let thread_id = ThreadId::new();
    store.push_notification(turn_started_notification(thread_id, "turn-1"));

    store.clear_active_turn_id();

    assert_eq!(store.active_turn_id(), None);
}

#[test]
fn idle_status_clears_cached_active_turn_without_turn_completed() {
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    let thread_id = ThreadId::new();
    store.push_notification(turn_started_notification(thread_id, "turn-1"));

    store.push_notification(ServerNotification::ThreadStatusChanged(
        praxis_app_gateway_protocol::ThreadStatusChangedNotification {
            thread_id: thread_id.to_string(),
            status: ThreadStatus::Idle,
        },
    ));

    assert_eq!(store.status, Some(ThreadStatus::Idle));
    assert_eq!(store.active_turn_id(), None);
}

#[test]
fn thread_event_store_rebase_preserves_resolved_request_state() {
    let thread_id = ThreadId::new();
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    store.push_request(exec_approval_request(
        thread_id,
        "turn-approval",
        "call-approval",
        /*approval_id*/ None,
    ));
    store.push_notification(ServerNotification::ServerRequestResolved(
        praxis_app_gateway_protocol::ServerRequestResolvedNotification {
            request_id: AppGatewayRequestId::Integer(1),
            thread_id: thread_id.to_string(),
        },
    ));

    store.rebase_buffer_after_session_refresh();

    let snapshot = store.snapshot();
    assert!(snapshot.events.is_empty());
    assert_eq!(store.has_pending_thread_approvals(), false);
}

#[test]
fn repeated_pending_request_replay_is_idempotent_by_request_id() {
    let thread_id = ThreadId::new();
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    let request = exec_approval_request(
        thread_id,
        "turn-approval",
        "call-approval",
        /*approval_id*/ None,
    );

    assert!(store.push_request(request.clone()));
    assert!(!store.push_request(request));

    assert_eq!(
        store
            .snapshot()
            .events
            .iter()
            .filter(|event| matches!(event, ThreadBufferedEvent::Request(_)))
            .count(),
        1
    );
    assert!(store.has_pending_thread_approvals());
}

#[test]
fn thread_event_store_rebase_preserves_hook_notifications() {
    let thread_id = ThreadId::new();
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    store.push_notification(hook_started_notification(thread_id, "turn-hook"));
    store.push_notification(hook_completed_notification(thread_id, "turn-hook"));

    store.rebase_buffer_after_session_refresh();

    let snapshot = store.snapshot();
    let hook_notifications = snapshot
        .events
        .into_iter()
        .map(|event| match event {
            ThreadBufferedEvent::Notification(notification) => {
                serde_json::to_value(notification).expect("hook notification should serialize")
            }
            other => panic!("expected buffered hook notification, saw: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        hook_notifications,
        vec![
            serde_json::to_value(hook_started_notification(thread_id, "turn-hook"))
                .expect("hook notification should serialize"),
            serde_json::to_value(hook_completed_notification(thread_id, "turn-hook"))
                .expect("hook notification should serialize"),
        ]
    );
}

#[test]
fn thread_event_store_rebase_preserves_uncovered_in_progress_delta() {
    let thread_id = ThreadId::new();
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    store.push_notification(agent_message_delta_notification(
        thread_id,
        "turn-live",
        "agent-live",
        "prefix",
    ));
    store.set_runtime_snapshot(
        test_thread_session(thread_id, PathBuf::from("/tmp/project")),
        vec![test_turn("turn-live", TurnStatus::InProgress, Vec::new())],
        ThreadStatus::Active {
            active_flags: vec![praxis_app_gateway_protocol::ThreadActiveFlag::Running],
        },
        None,
    );

    store.rebase_buffer_after_session_refresh();

    assert!(matches!(
        store.snapshot().events.as_slice(),
        [ThreadBufferedEvent::Notification(
            ServerNotification::AgentMessageDelta(notification)
        )] if notification.delta == "prefix"
    ));
}

#[test]
fn completed_turn_snapshot_covers_delta_with_different_item_id() {
    let thread_id = ThreadId::new();
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    store.push_notification(agent_message_delta_notification(
        thread_id,
        "turn-complete",
        "live-agent-id",
        "duplicate",
    ));
    store.set_runtime_snapshot(
        test_thread_session(thread_id, PathBuf::from("/tmp/project")),
        vec![test_turn(
            "turn-complete",
            TurnStatus::Completed,
            vec![ThreadItem::AgentMessage {
                id: "history-builder-item-1".to_string(),
                text: "complete answer".to_string(),
                phase: None,
                memory_citation: None,
            }],
        )],
        ThreadStatus::Idle,
        None,
    );

    store.rebase_buffer_after_session_refresh();

    assert!(store.snapshot().events.is_empty());
}

#[test]
fn in_progress_command_snapshot_covers_aggregated_output_delta() {
    let thread_id = ThreadId::new();
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    store.push_notification(ServerNotification::CommandExecutionOutputDelta(
        praxis_app_gateway_protocol::CommandExecutionOutputDeltaNotification {
            thread_id: thread_id.to_string(),
            turn_id: "turn-live".to_string(),
            item_id: "command-live".to_string(),
            delta: "already captured\n".to_string(),
        },
    ));
    store.set_runtime_snapshot(
        test_thread_session(thread_id, PathBuf::from("/tmp/project")),
        vec![test_turn(
            "turn-live",
            TurnStatus::InProgress,
            vec![ThreadItem::CommandExecution {
                id: "command-live".to_string(),
                command: "echo captured".to_string(),
                cwd: PathBuf::from("/tmp/project"),
                process_id: Some("process-live".to_string()),
                source: praxis_app_gateway_protocol::CommandExecutionSource::Agent,
                status: praxis_app_gateway_protocol::CommandExecutionStatus::InProgress,
                command_actions: Vec::new(),
                aggregated_output: Some("already captured\n".to_string()),
                exit_code: None,
                duration_ms: None,
            }],
        )],
        ThreadStatus::Active {
            active_flags: vec![praxis_app_gateway_protocol::ThreadActiveFlag::Running],
        },
        None,
    );

    store.rebase_buffer_after_session_refresh();

    assert!(store.snapshot().events.is_empty());
}

#[test]
fn thread_closed_clears_cached_runtime_state() {
    let thread_id = ThreadId::new();
    let mut store = ThreadEventStore::new(/*capacity*/ 8);
    store.set_runtime_snapshot(
        test_thread_session(thread_id, PathBuf::from("/tmp/project")),
        vec![test_turn("turn-live", TurnStatus::InProgress, Vec::new())],
        ThreadStatus::Active {
            active_flags: vec![praxis_app_gateway_protocol::ThreadActiveFlag::Running],
        },
        Some(praxis_app_gateway_protocol::ThreadControlState {
            controller: praxis_app_gateway_protocol::ThreadController {
                kind: praxis_app_gateway_protocol::ThreadControllerKind::External,
                id: "controller".to_string(),
                label: Some("Controller".to_string()),
                rank: None,
            },
            reason: Some("test lock".to_string()),
            read_only: true,
            acquired_at: 1,
            updated_at: 1,
        }),
    );

    store.push_notification(thread_closed_notification(thread_id));

    assert_eq!(store.status, Some(ThreadStatus::NotLoaded));
    assert_eq!(store.control_state, None);
    assert_eq!(store.active_turn_id(), None);
}

#[tokio::test]
async fn switch_replay_watermark_rejects_queued_store_duplicate() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    let mut channel = ThreadEventChannel::new(/*capacity*/ 8);
    let event = turn_started_notification(thread_id, "turn-live");
    let sequence = {
        let mut store = channel.store.lock().await;
        store.push_notification(event.clone())
    };
    channel
        .sender
        .send(ThreadEventEnvelope {
            sequence,
            event: ThreadBufferedEvent::Notification(event),
        })
        .expect("queued live event should fit");
    app.thread_event_channels.insert(thread_id, channel);

    let (mut receiver, snapshot) = app
        .activate_thread_for_replay(thread_id)
        .await
        .expect("thread should activate");
    app.active_thread_id = Some(thread_id);
    assert_eq!(snapshot.events.len(), 1);

    let queued = receiver
        .try_recv()
        .expect("the old receiver should still contain the snapshotted event");
    assert!(!app.active_thread_event_is_after_replay(&queued));
}

#[test]
fn live_delivery_stays_fifo_beyond_replay_capacity() {
    let mut channel = ThreadEventChannel::new(/*capacity*/ 2);
    for sequence in 1..=8 {
        channel
            .sender
            .send(ThreadEventEnvelope {
                sequence,
                event: ThreadBufferedEvent::Notification(ServerNotification::ConfigWarning(
                    praxis_app_gateway_protocol::ConfigWarningNotification {
                        summary: format!("event-{sequence}"),
                        details: None,
                        path: None,
                        range: None,
                    },
                )),
            })
            .expect("live delivery receiver should remain open");
    }

    let receiver = channel
        .receiver
        .as_mut()
        .expect("test channel should own its receiver");
    let sequences = std::iter::from_fn(|| receiver.try_recv().ok())
        .map(|envelope| envelope.sequence)
        .collect::<Vec<_>>();
    assert_eq!(sequences, (1..=8).collect::<Vec<_>>());
}

#[test]
fn build_feedback_upload_params_includes_thread_id_and_rollout_path() {
    let thread_id = ThreadId::new();
    let rollout_path = PathBuf::from("/tmp/rollout.jsonl");

    let params = build_feedback_upload_params(
        Some(thread_id),
        Some(rollout_path.clone()),
        FeedbackCategory::SafetyCheck,
        Some("needs follow-up".to_string()),
        /*include_logs*/ true,
    );

    assert_eq!(params.classification, "safety_check");
    assert_eq!(params.reason, Some("needs follow-up".to_string()));
    assert_eq!(params.thread_id, Some(thread_id.to_string()));
    assert_eq!(params.include_logs, true);
    assert_eq!(params.extra_log_files, Some(vec![rollout_path]));
}

#[test]
fn build_feedback_upload_params_omits_rollout_path_without_logs() {
    let params = build_feedback_upload_params(
        /*origin_thread_id*/ None,
        Some(PathBuf::from("/tmp/rollout.jsonl")),
        FeedbackCategory::GoodResult,
        /*reason*/ None,
        /*include_logs*/ false,
    );

    assert_eq!(params.classification, "good_result");
    assert_eq!(params.reason, None);
    assert_eq!(params.thread_id, None);
    assert_eq!(params.include_logs, false);
    assert_eq!(params.extra_log_files, None);
}

#[tokio::test]
async fn feedback_submission_without_thread_emits_error_history_cell() {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;

    app.handle_feedback_submitted(
        /*origin_thread_id*/ None,
        FeedbackCategory::Bug,
        /*include_logs*/ true,
        Err("boom".to_string()),
    )
    .await;

    let cell = match app_event_rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => cell,
        other => panic!("expected feedback error history cell, saw {other:?}"),
    };
    assert_eq!(
        lines_to_single_string(&cell.display_lines(/*width*/ 120)),
        "■ Failed to upload feedback: boom"
    );
}

#[tokio::test]
async fn feedback_submission_for_inactive_thread_replays_into_origin_thread() {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let origin_thread_id = ThreadId::new();
    let active_thread_id = ThreadId::new();
    let origin_session = test_thread_session(origin_thread_id, PathBuf::from("/tmp/origin"));
    let active_session = test_thread_session(active_thread_id, PathBuf::from("/tmp/active"));
    app.thread_event_channels.insert(
        origin_thread_id,
        ThreadEventChannel::new_with_session(
            THREAD_EVENT_REPLAY_CAPACITY,
            origin_session.clone(),
            Vec::new(),
        ),
    );
    app.thread_event_channels.insert(
        active_thread_id,
        ThreadEventChannel::new_with_session(
            THREAD_EVENT_REPLAY_CAPACITY,
            active_session.clone(),
            Vec::new(),
        ),
    );
    app.activate_thread_channel(active_thread_id).await;
    app.chat_widget.handle_thread_session(active_session);
    while app_event_rx.try_recv().is_ok() {}

    app.handle_feedback_submitted(
        Some(origin_thread_id),
        FeedbackCategory::Bug,
        /*include_logs*/ true,
        Ok("uploaded-thread".to_string()),
    )
    .await;

    assert_matches!(
        app_event_rx.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    );

    let snapshot = {
        let channel = app
            .thread_event_channels
            .get(&origin_thread_id)
            .expect("origin thread channel should exist");
        let store = channel.store.lock().await;
        assert!(matches!(
            store.buffer.back().map(|envelope| &envelope.event),
            Some(ThreadBufferedEvent::FeedbackSubmission(_))
        ));
        store.snapshot()
    };

    app.replay_thread_snapshot(snapshot, /*resume_restored_queue*/ false);

    let mut rendered_cells = Vec::new();
    while let Ok(event) = app_event_rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = event {
            rendered_cells.push(lines_to_single_string(&cell.display_lines(/*width*/ 120)));
        }
    }
    assert!(rendered_cells.iter().any(|cell| {
        cell.contains("• Feedback uploaded. Please open an issue using the following URL:")
            && cell.contains("uploaded-thread")
    }));
}

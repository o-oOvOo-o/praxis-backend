use super::*;

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

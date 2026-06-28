use super::*;

#[tokio::test]
#[traced_test]
async fn process_sse_emits_failed_event_on_parse_error() {
    let server = start_mock_server().await;

    mount_sse_once(&server, "data: not-json\n\n".to_string()).await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event")
                    && line.contains("error.message")
                    && line.contains("expected ident at line 1 column 2")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing praxis.sse_event".to_string()))
    });
}

#[tokio::test]
#[traced_test]
async fn process_sse_records_failed_event_when_stream_closes_without_completed() {
    let server = start_mock_server().await;

    mount_sse_once(&server, sse(vec![ev_assistant_message("id", "hi")])).await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event")
                    && line.contains("error.message")
                    && line.contains("stream closed before response.completed")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing praxis.sse_event".to_string()))
    });
}

#[tokio::test]
#[traced_test]
async fn process_sse_failed_event_records_response_error_message() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![serde_json::json!({
            "type": "response.failed",
            "response": {
                "error": {
                    "message": "boom",
                    "code": "bad"
                }
            }
        })]),
    )
    .await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event")
                    && line.contains("event.kind=response.failed")
                    && line.contains("error.message")
                    && line.contains("boom")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing praxis.sse_event".to_string()))
    });
}

#[tokio::test]
#[traced_test]
async fn process_sse_failed_event_logs_parse_error() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![serde_json::json!({
            "type": "response.failed",
            "response": {
                "error": "not-an-object"
            }
        })]),
    )
    .await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event") && line.contains("event.kind=response.failed")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing praxis.sse_event".to_string()))
    });
}

#[tokio::test]
#[traced_test]
async fn process_sse_failed_event_logs_missing_error() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![serde_json::json!({
            "type": "response.failed",
            "response": {}
        })]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event") && line.contains("event.kind=response.failed")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing praxis.sse_event".to_string()))
    });
}

#[tokio::test]
#[traced_test]
async fn process_sse_failed_event_logs_response_completed_parse_error() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![serde_json::json!({
            "type": "response.completed",
            "response": {}
        })]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event")
                    && line.contains("event.kind=response.completed")
                    && line.contains("error.message")
                    && line.contains("failed to parse ResponseCompleted")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing praxis.sse_event".to_string()))
    });
}

#[tokio::test]
#[traced_test]
async fn process_sse_emits_completed_telemetry() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![serde_json::json!({
            "type": "response.completed",
            "response": {
                "id": "resp1",
                "usage": {
                    "input_tokens": 3,
                    "input_tokens_details": { "cached_tokens": 1 },
                    "output_tokens": 5,
                    "output_tokens_details": { "reasoning_tokens": 2 },
                    "total_tokens": 9
                }
            }
        })]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis().build(&server).await.unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| {
                line.contains("praxis.sse_event")
                    && line.contains("event.kind=response.completed")
                    && line.contains("input_token_count=3")
                    && line.contains("output_token_count=5")
                    && line.contains("cached_token_count=1")
                    && line.contains("reasoning_token_count=2")
                    && line.contains("tool_token_count=9")
            })
            .map(|_| Ok(()))
            .unwrap_or(Err("missing response.completed telemetry".to_string()))
    });
}

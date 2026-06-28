#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[traced_test]
async fn responses_websocket_emits_websocket_telemetry_events() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness = websocket_harness(&server).await;
    harness.session_telemetry.reset_runtime_metrics();
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);

    stream_until_complete(&mut client_session, &harness, &prompt).await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let summary = harness
        .session_telemetry
        .runtime_metrics_summary()
        .expect("runtime metrics summary");
    assert_eq!(summary.api_calls.count, 0);
    assert_eq!(summary.streaming_events.count, 0);
    assert_eq!(summary.websocket_calls.count, 1);
    assert_eq!(summary.websocket_events.count, 2);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_includes_timing_metrics_header_when_runtime_metrics_enabled() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        serde_json::json!({
            "type": "responsesapi.websocket_timing",
            "timing_metrics": {
                "responses_duration_excl_engine_and_client_tool_time_ms": 120,
                "engine_service_total_ms": 450,
                "engine_iapi_ttft_total_ms": 310,
                "engine_service_ttft_total_ms": 340,
                "engine_iapi_tbt_across_engine_calls_ms": 220,
                "engine_service_tbt_across_engine_calls_ms": 260
            }
        }),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness =
        websocket_harness_with_runtime_metrics(&server, /*runtime_metrics_enabled*/ true).await;
    harness.session_telemetry.reset_runtime_metrics();
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);

    stream_until_complete(&mut client_session, &harness, &prompt).await;
    tokio::time::sleep(Duration::from_millis(10)).await;

    let handshake = server.single_handshake();
    assert_eq!(
        handshake.header(X_RESPONSESAPI_INCLUDE_TIMING_METRICS_HEADER),
        Some("true".to_string())
    );

    let summary = harness
        .session_telemetry
        .runtime_metrics_summary()
        .expect("runtime metrics summary");
    assert_eq!(summary.responses_api_overhead_ms, 120);
    assert_eq!(summary.responses_api_inference_time_ms, 450);
    assert_eq!(summary.responses_api_engine_iapi_ttft_ms, 310);
    assert_eq!(summary.responses_api_engine_service_ttft_ms, 340);
    assert_eq!(summary.responses_api_engine_iapi_tbt_ms, 220);
    assert_eq!(summary.responses_api_engine_service_tbt_ms, 260);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_omits_timing_metrics_header_when_runtime_metrics_disabled() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness =
        websocket_harness_with_runtime_metrics(&server, /*runtime_metrics_enabled*/ false).await;
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);

    stream_until_complete(&mut client_session, &harness, &prompt).await;

    let handshake = server.single_handshake();
    assert_eq!(
        handshake.header(X_RESPONSESAPI_INCLUDE_TIMING_METRICS_HEADER),
        None
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_emits_reasoning_included_event() {
    skip_if_no_network!();

    let server = start_websocket_server_with_headers(vec![WebSocketConnectionConfig {
        requests: vec![vec![ev_response_created("resp-1"), ev_completed("resp-1")]],
        response_headers: vec![("X-Reasoning-Included".to_string(), "true".to_string())],
        accept_delay: None,
        close_after_requests: true,
    }])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);

    let mut stream = client_session
        .stream(
            &prompt,
            &harness.model_info,
            &harness.session_telemetry,
            harness.effort,
            harness.summary,
            /*service_tier*/ None,
            /*turn_metadata_header*/ None,
        )
        .await
        .expect("websocket stream failed");

    let mut saw_reasoning_included = false;
    while let Some(event) = stream.next().await {
        match event.expect("event") {
            ResponseEvent::ServerReasoningIncluded(true) => {
                saw_reasoning_included = true;
            }
            ResponseEvent::Completed { .. } => break,
            _ => {}
        }
    }

    assert!(saw_reasoning_included);
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_emits_rate_limit_events() {
    skip_if_no_network!();

    let rate_limit_event = json!({
        "type": "codex.rate_limits",
        "plan_type": "plus",
        "rate_limits": {
            "allowed": true,
            "limit_reached": false,
            "primary": {
                "used_percent": 42,
                "window_minutes": 60,
                "reset_at": 1700000000
            },
            "secondary": null
        },
        "code_review_rate_limits": null,
        "credits": {
            "has_credits": true,
            "unlimited": false,
            "balance": "123"
        },
        "promo": null
    });

    let server = start_websocket_server_with_headers(vec![WebSocketConnectionConfig {
        requests: vec![vec![
            rate_limit_event,
            ev_response_created("resp-1"),
            ev_completed("resp-1"),
        ]],
        response_headers: vec![
            ("X-Models-Etag".to_string(), "etag-123".to_string()),
            ("X-Reasoning-Included".to_string(), "true".to_string()),
        ],
        accept_delay: None,
        close_after_requests: true,
    }])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);

    let mut stream = client_session
        .stream(
            &prompt,
            &harness.model_info,
            &harness.session_telemetry,
            harness.effort,
            harness.summary,
            /*service_tier*/ None,
            /*turn_metadata_header*/ None,
        )
        .await
        .expect("websocket stream failed");

    let mut saw_rate_limits = None;
    let mut saw_models_etag = None;
    let mut saw_reasoning_included = false;

    while let Some(event) = stream.next().await {
        match event.expect("event") {
            ResponseEvent::RateLimits(snapshot) => {
                saw_rate_limits = Some(snapshot);
            }
            ResponseEvent::ModelsEtag(etag) => {
                saw_models_etag = Some(etag);
            }
            ResponseEvent::ServerReasoningIncluded(true) => {
                saw_reasoning_included = true;
            }
            ResponseEvent::Completed { .. } => break,
            _ => {}
        }
    }

    let rate_limits = saw_rate_limits.expect("missing rate limits");
    let primary = rate_limits.primary.expect("missing primary window");
    assert_eq!(primary.used_percent, 42.0);
    assert_eq!(primary.window_minutes, Some(60));
    assert_eq!(primary.resets_at, Some(1_700_000_000));
    assert_eq!(rate_limits.plan_type, Some(PlanType::Plus));
    let credits = rate_limits.credits.expect("missing credits");
    assert!(credits.has_credits);
    assert!(!credits.unlimited);
    assert_eq!(credits.balance.as_deref(), Some("123"));
    assert_eq!(saw_models_etag.as_deref(), Some("etag-123"));
    assert!(saw_reasoning_included);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_usage_limit_error_emits_rate_limit_event() {
    skip_if_no_network!();

    let usage_limit_error = json!({
        "type": "error",
        "status": 429,
        "error": {
            "type": "usage_limit_reached",
            "message": "The usage limit has been reached",
            "plan_type": "pro",
            "resets_at": 1704067242,
            "resets_in_seconds": 1234
        },
        "headers": {
            "x-praxis-primary-used-percent": "100.0",
            "x-praxis-secondary-used-percent": "87.5",
            "x-praxis-primary-over-secondary-limit-percent": "95.0",
            "x-praxis-primary-window-minutes": "15",
            "x-praxis-secondary-window-minutes": "60"
        }
    });

    let server = start_websocket_server(vec![vec![
        vec![
            ev_response_created("resp-prewarm"),
            ev_completed("resp-prewarm"),
        ],
        vec![usage_limit_error],
    ]])
    .await;
    let mut builder = test_praxis().with_config(|config| {
        config.model_provider.request_max_retries = Some(0);
        config.model_provider.stream_max_retries = Some(0);
    });
    let test = builder
        .build_with_websocket_server(&server)
        .await
        .expect("build websocket praxis thread");

    let submission_id = test
        .thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .expect("submission should succeed while emitting usage limit error events");

    let token_event =
        wait_for_event(&test.thread, |msg| matches!(msg, EventMsg::TokenCount(_))).await;
    let EventMsg::TokenCount(event) = token_event else {
        unreachable!();
    };

    let event_json = serde_json::to_value(&event).expect("serialize token count event");
    pretty_assertions::assert_eq!(
        event_json,
        json!({
            "info": null,
            "rate_limits": {
                "limit_id": "codex",
                "limit_name": null,
                "primary": {
                    "used_percent": 100.0,
                    "window_minutes": 15,
                    "resets_at": null
                },
                "secondary": {
                    "used_percent": 87.5,
                    "window_minutes": 60,
                    "resets_at": null
                },
                "credits": null,
                "plan_type": null
            }
        })
    );

    let error_event = wait_for_event(&test.thread, |msg| matches!(msg, EventMsg::Error(_))).await;
    let EventMsg::Error(error_event) = error_event else {
        unreachable!();
    };
    assert!(
        error_event.message.to_lowercase().contains("usage limit"),
        "unexpected error message for submission {submission_id}: {}",
        error_event.message
    );

    server.shutdown().await;
}

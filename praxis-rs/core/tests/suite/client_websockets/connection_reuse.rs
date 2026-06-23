#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_streams_request() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);

    stream_until_complete(&mut client_session, &harness, &prompt).await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 1);
    let body = connection.first().expect("missing request").body_json();

    assert_eq!(body["type"].as_str(), Some("response.create"));
    assert_eq!(body["model"].as_str(), Some(MODEL));
    assert_eq!(body["stream"], serde_json::Value::Bool(true));
    assert_eq!(body["input"].as_array().map(Vec::len), Some(1));
    let handshake = server.single_handshake();
    assert_eq!(
        handshake.header(OPENAI_BETA_HEADER),
        Some(WS_V2_BETA_HEADER_VALUE.to_string())
    );
    assert_eq!(
        handshake.header(X_CLIENT_REQUEST_ID_HEADER),
        Some(harness.conversation_id.to_string())
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_streams_without_feature_flag_when_provider_supports_websockets() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness = websocket_harness_with_options(&server, /*runtime_metrics_enabled*/ false).await;
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);

    stream_until_complete(&mut client_session, &harness, &prompt).await;

    assert_eq!(server.handshakes().len(), 1);
    assert_eq!(server.single_connection().len(), 1);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_reuses_connection_with_per_turn_trace_payloads() {
    skip_if_no_network!();

    let _trace_test_context = install_test_tracing("client-websocket-test");

    let server = start_websocket_server(vec![vec![
        vec![ev_response_created("resp-1"), ev_completed("resp-1")],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness(&server).await;
    let prompt_one = prompt_with_input(vec![message_item("hello")]);
    let prompt_two = prompt_with_input(vec![message_item("again")]);

    let first_trace = {
        let mut client_session = harness.client.new_session();
        async {
            let expected_trace =
                current_span_w3c_trace_context().expect("current span should have trace context");
            stream_until_complete(&mut client_session, &harness, &prompt_one).await;
            expected_trace
        }
        .instrument(tracing::info_span!("client.websocket.turn_one"))
        .await
    };

    let second_trace = {
        let mut client_session = harness.client.new_session();
        async {
            let expected_trace =
                current_span_w3c_trace_context().expect("current span should have trace context");
            stream_until_complete(&mut client_session, &harness, &prompt_two).await;
            expected_trace
        }
        .instrument(tracing::info_span!("client.websocket.turn_two"))
        .await
    };

    assert_eq!(server.handshakes().len(), 1);
    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);

    let first_request = connection
        .first()
        .expect("missing first request")
        .body_json();
    let second_request = connection
        .get(1)
        .expect("missing second request")
        .body_json();
    assert_request_trace_matches(&first_request, &first_trace);
    assert_request_trace_matches(&second_request, &second_trace);

    let first_traceparent = first_request["client_metadata"]
        [WS_REQUEST_HEADER_TRACEPARENT_CLIENT_METADATA_KEY]
        .as_str()
        .expect("missing first traceparent");
    let second_traceparent = second_request["client_metadata"]
        [WS_REQUEST_HEADER_TRACEPARENT_CLIENT_METADATA_KEY]
        .as_str()
        .expect("missing second traceparent");
    assert_ne!(first_traceparent, second_traceparent);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_preconnect_does_not_replace_turn_trace_payload() {
    skip_if_no_network!();

    let _trace_test_context = install_test_tracing("client-websocket-test");

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    client_session
        .preconnect_websocket(&harness.session_telemetry, &harness.model_info)
        .await
        .expect("websocket preconnect failed");
    let prompt = prompt_with_input(vec![message_item("hello")]);

    let expected_trace = async {
        let expected_trace =
            current_span_w3c_trace_context().expect("current span should have trace context");
        stream_until_complete(&mut client_session, &harness, &prompt).await;
        expected_trace
    }
    .instrument(tracing::info_span!("client.websocket.request"))
    .await;

    assert_eq!(server.handshakes().len(), 1);
    let connection = server.single_connection();
    assert_eq!(connection.len(), 1);
    let request = connection.first().expect("missing request").body_json();
    assert_request_trace_matches(&request, &expected_trace);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_preconnect_reuses_connection() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    client_session
        .preconnect_websocket(&harness.session_telemetry, &harness.model_info)
        .await
        .expect("websocket preconnect failed");
    let prompt = prompt_with_input(vec![message_item("hello")]);
    stream_until_complete(&mut client_session, &harness, &prompt).await;

    assert_eq!(server.handshakes().len(), 1);
    assert_eq!(server.single_connection().len(), 1);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_request_prewarm_reuses_connection() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![ev_response_created("warm-1"), ev_completed("warm-1")],
        vec![ev_response_created("resp-1"), ev_completed("resp-1")],
    ]])
    .await;

    let harness = websocket_harness_with_options(&server, /*runtime_metrics_enabled*/ true).await;
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);
    client_session
        .prewarm_websocket(
            &prompt,
            &harness.model_info,
            &harness.session_telemetry,
            harness.effort,
            harness.summary,
            /*service_tier*/ None,
            /*turn_metadata_header*/ None,
        )
        .await
        .expect("websocket prewarm failed");
    stream_until_complete(&mut client_session, &harness, &prompt).await;

    assert_eq!(server.handshakes().len(), 1);
    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let warmup = connection
        .first()
        .expect("missing warmup request")
        .body_json();
    let follow_up = connection
        .get(1)
        .expect("missing follow-up request")
        .body_json();

    assert_eq!(warmup["type"].as_str(), Some("response.create"));
    assert_eq!(warmup["generate"].as_bool(), Some(false));
    assert_eq!(warmup["tools"], serde_json::json!([]));
    assert_eq!(follow_up["type"].as_str(), Some("response.create"));
    assert_eq!(follow_up["previous_response_id"].as_str(), Some("warm-1"));
    assert_eq!(follow_up["input"], serde_json::json!([]));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_reuses_connection_after_session_drop() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![ev_response_created("resp-1"), ev_completed("resp-1")],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness(&server).await;
    let prompt_one = prompt_with_input(vec![message_item("hello")]);
    let prompt_two = prompt_with_input(vec![message_item("again")]);

    {
        let mut client_session = harness.client.new_session();
        stream_until_complete(&mut client_session, &harness, &prompt_one).await;
    }

    let mut client_session = harness.client.new_session();
    stream_until_complete(&mut client_session, &harness, &prompt_two).await;

    assert_eq!(server.handshakes().len(), 1);
    assert_eq!(server.single_connection().len(), 2);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_preconnect_is_reused_even_with_header_changes() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![vec![
        ev_response_created("resp-1"),
        ev_completed("resp-1"),
    ]]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    client_session
        .preconnect_websocket(&harness.session_telemetry, &harness.model_info)
        .await
        .expect("websocket preconnect failed");
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

    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }

    assert_eq!(server.handshakes().len(), 1);
    assert_eq!(server.single_connection().len(), 1);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_request_prewarm_is_reused_even_with_header_changes() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![ev_response_created("warm-1"), ev_completed("warm-1")],
        vec![ev_response_created("resp-1"), ev_completed("resp-1")],
    ]])
    .await;

    let harness = websocket_harness_with_options(&server, /*runtime_metrics_enabled*/ true).await;
    let mut client_session = harness.client.new_session();
    let prompt = prompt_with_input(vec![message_item("hello")]);
    client_session
        .prewarm_websocket(
            &prompt,
            &harness.model_info,
            &harness.session_telemetry,
            harness.effort,
            harness.summary,
            /*service_tier*/ None,
            /*turn_metadata_header*/ None,
        )
        .await
        .expect("websocket prewarm failed");
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

    while let Some(event) = stream.next().await {
        if matches!(event, Ok(ResponseEvent::Completed { .. })) {
            break;
        }
    }

    assert_eq!(server.handshakes().len(), 1);
    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let warmup = connection
        .first()
        .expect("missing warmup request")
        .body_json();
    let follow_up = connection
        .get(1)
        .expect("missing follow-up request")
        .body_json();
    assert_eq!(warmup["type"].as_str(), Some("response.create"));
    assert_eq!(warmup["generate"].as_bool(), Some(false));
    assert_eq!(warmup["tools"], serde_json::json!([]));
    assert_eq!(follow_up["type"].as_str(), Some("response.create"));
    assert_eq!(follow_up["previous_response_id"].as_str(), Some("warm-1"));
    assert_eq!(follow_up["input"], serde_json::json!([]));

    server.shutdown().await;
}

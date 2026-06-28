#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_invalid_request_error_with_status_is_forwarded() {
    skip_if_no_network!();

    let invalid_request_error = json!({
        "type": "error",
        "status": 400,
        "error": {
            "type": "invalid_request_error",
            "message": "Model 'castor-raikou-0205-ev3' does not support image inputs."
        }
    });

    let server = start_websocket_server(vec![vec![
        vec![
            ev_response_created("resp-prewarm"),
            ev_completed("resp-prewarm"),
        ],
        vec![invalid_request_error],
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
        .expect("submission should succeed while emitting invalid request events");

    let error_event = wait_for_event(&test.thread, |msg| matches!(msg, EventMsg::Error(_))).await;
    let EventMsg::Error(error_event) = error_event else {
        unreachable!();
    };
    assert!(
        error_event
            .message
            .to_lowercase()
            .contains("does not support image inputs"),
        "unexpected error message for submission {submission_id}: {}",
        error_event.message
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_connection_limit_error_reconnects_and_completes() {
    skip_if_no_network!();

    let websocket_connection_limit_error = json!({
        "type": "error",
        "status": 400,
        "error": {
            "type": "invalid_request_error",
            "code": "websocket_connection_limit_reached",
            "message": "Responses websocket connection limit reached (60 minutes). Create a new websocket connection to continue."
        }
    });

    let server = start_websocket_server(vec![
        vec![vec![websocket_connection_limit_error]],
        vec![vec![ev_response_created("resp-1"), ev_completed("resp-1")]],
    ])
    .await;
    let mut builder = test_praxis().with_config(|config| {
        config.model_provider.request_max_retries = Some(0);
        config.model_provider.stream_max_retries = Some(1);
    });
    let test = builder
        .build_with_websocket_server(&server)
        .await
        .expect("build websocket praxis thread");

    test.submit_turn("hello")
        .await
        .expect("submission should reconnect after websocket connection limit error");

    let total_websocket_requests: usize = server.connections().iter().map(Vec::len).sum();
    assert_eq!(total_websocket_requests, 2);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_uses_incremental_create_on_prefix() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "assistant output"),
            ev_completed("resp-1"),
        ],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let prompt_one = prompt_with_input(vec![message_item("hello")]);
    let prompt_two = prompt_with_input(vec![
        message_item("hello"),
        assistant_message_item("msg-1", "assistant output"),
        message_item("second"),
    ]);

    stream_until_complete(&mut client_session, &harness, &prompt_one).await;
    stream_until_complete(&mut client_session, &harness, &prompt_two).await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let first = connection.first().expect("missing request").body_json();
    let second = connection.get(1).expect("missing request").body_json();

    assert_eq!(first["type"].as_str(), Some("response.create"));
    assert_eq!(first["model"].as_str(), Some(MODEL));
    assert_eq!(first["stream"], serde_json::Value::Bool(true));
    assert_eq!(first["input"].as_array().map(Vec::len), Some(1));
    assert_eq!(second["type"].as_str(), Some("response.create"));
    assert_eq!(second["previous_response_id"].as_str(), Some("resp-1"));
    assert_eq!(
        second["input"],
        serde_json::to_value(&prompt_two.input[2..]).expect("serialize incremental items")
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_forwards_turn_metadata_on_initial_and_incremental_create() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "assistant output"),
            ev_completed("resp-1"),
        ],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let first_turn_metadata = r#"{"turn_id":"turn-123","sandbox":"workspace-write"}"#;
    let enriched_turn_metadata = r#"{"turn_id":"turn-123","sandbox":"workspace-write","workspaces":[{"root_path":"/tmp/repo","latest_git_commit_hash":"abc123","associated_remote_urls":["git@github.com:cunning3d/praxis.git"],"has_changes":true}]}"#;
    let prompt_one = prompt_with_input(vec![message_item("hello")]);
    let prompt_two = prompt_with_input(vec![
        message_item("hello"),
        assistant_message_item("msg-1", "assistant output"),
        message_item("second"),
    ]);

    stream_until_complete_with_turn_metadata(
        &mut client_session,
        &harness,
        &prompt_one,
        /*service_tier*/ None,
        Some(first_turn_metadata),
    )
    .await;
    stream_until_complete_with_turn_metadata(
        &mut client_session,
        &harness,
        &prompt_two,
        /*service_tier*/ None,
        Some(enriched_turn_metadata),
    )
    .await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let first = connection.first().expect("missing request").body_json();
    let second = connection.get(1).expect("missing request").body_json();

    assert_eq!(first["type"].as_str(), Some("response.create"));
    assert_eq!(
        first["client_metadata"]["x-praxis-turn-metadata"].as_str(),
        Some(first_turn_metadata)
    );
    assert_eq!(second["type"].as_str(), Some("response.create"));
    assert_eq!(second["previous_response_id"].as_str(), Some("resp-1"));
    assert_eq!(
        second["client_metadata"]["x-praxis-turn-metadata"].as_str(),
        Some(enriched_turn_metadata)
    );

    let first_metadata: serde_json::Value =
        serde_json::from_str(first_turn_metadata).expect("first metadata should be valid json");
    let second_metadata: serde_json::Value = serde_json::from_str(enriched_turn_metadata)
        .expect("enriched metadata should be valid json");

    assert_eq!(first_metadata["turn_id"].as_str(), Some("turn-123"));
    assert_eq!(second_metadata["turn_id"].as_str(), Some("turn-123"));
    assert_eq!(
        second_metadata["workspaces"][0]["has_changes"].as_bool(),
        Some(true)
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_uses_previous_response_id_when_prefix_after_completed() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "assistant output"),
            ev_completed("resp-1"),
        ],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let prompt_one = prompt_with_input(vec![message_item("hello")]);
    let prompt_two = prompt_with_input(vec![
        message_item("hello"),
        assistant_message_item("msg-1", "assistant output"),
        message_item("second"),
    ]);

    stream_until_complete(&mut client_session, &harness, &prompt_one).await;
    stream_until_complete(&mut client_session, &harness, &prompt_two).await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let second = connection.get(1).expect("missing request").body_json();

    assert_eq!(second["type"].as_str(), Some("response.create"));
    assert_eq!(second["previous_response_id"].as_str(), Some("resp-1"));
    assert_eq!(
        second["input"],
        serde_json::to_value(&prompt_two.input[2..]).expect("serialize incremental input")
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_creates_on_non_prefix() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![ev_response_created("resp-1"), ev_completed("resp-1")],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let prompt_one = prompt_with_input(vec![message_item("hello")]);
    let prompt_two = prompt_with_input(vec![message_item("different")]);

    stream_until_complete(&mut client_session, &harness, &prompt_one).await;
    stream_until_complete(&mut client_session, &harness, &prompt_two).await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let second = connection.get(1).expect("missing request").body_json();

    assert_eq!(second["type"].as_str(), Some("response.create"));
    assert_eq!(second["model"].as_str(), Some(MODEL));
    assert_eq!(second["stream"], serde_json::Value::Bool(true));
    assert_eq!(
        second["input"],
        serde_json::to_value(&prompt_two.input).unwrap()
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_creates_when_non_input_request_fields_change() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![ev_response_created("resp-1"), ev_completed("resp-1")],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness(&server).await;
    let mut client_session = harness.client.new_session();
    let prompt_one =
        prompt_with_input_and_instructions(vec![message_item("hello")], "base instructions one");
    let prompt_two = prompt_with_input_and_instructions(
        vec![message_item("hello"), message_item("second")],
        "base instructions two",
    );

    stream_until_complete(&mut client_session, &harness, &prompt_one).await;
    stream_until_complete(&mut client_session, &harness, &prompt_two).await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let second = connection.get(1).expect("missing request").body_json();

    assert_eq!(second["type"].as_str(), Some("response.create"));
    assert_eq!(second.get("previous_response_id"), None);
    assert_eq!(
        second["input"],
        serde_json::to_value(&prompt_two.input).expect("serialize full input")
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_v2_creates_with_previous_response_id_on_prefix() {
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "assistant output"),
            ev_completed("resp-1"),
        ],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness_with_v2(&server, /*runtime_metrics_enabled*/ true).await;
    let mut session = harness.client.new_session();
    let prompt_one = prompt_with_input(vec![message_item("hello")]);
    let prompt_two = prompt_with_input(vec![
        message_item("hello"),
        assistant_message_item("msg-1", "assistant output"),
        message_item("second"),
    ]);

    stream_until_complete(&mut session, &harness, &prompt_one).await;
    stream_until_complete(&mut session, &harness, &prompt_two).await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let first = connection.first().expect("missing request").body_json();
    let second = connection.get(1).expect("missing request").body_json();

    assert_eq!(first["type"].as_str(), Some("response.create"));
    assert_eq!(second["type"].as_str(), Some("response.create"));
    assert_eq!(second["previous_response_id"].as_str(), Some("resp-1"));
    assert_eq!(
        second["input"],
        serde_json::to_value(&prompt_two.input[2..]).unwrap()
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_websocket_v2_creates_without_previous_response_id_when_non_input_fields_change()
{
    skip_if_no_network!();

    let server = start_websocket_server(vec![vec![
        vec![ev_response_created("resp-1"), ev_completed("resp-1")],
        vec![ev_response_created("resp-2"), ev_completed("resp-2")],
    ]])
    .await;

    let harness = websocket_harness_with_v2(&server, /*runtime_metrics_enabled*/ true).await;
    let mut session = harness.client.new_session();
    let prompt_one =
        prompt_with_input_and_instructions(vec![message_item("hello")], "base instructions one");
    let prompt_two = prompt_with_input_and_instructions(
        vec![message_item("hello"), message_item("second")],
        "base instructions two",
    );

    stream_until_complete(&mut session, &harness, &prompt_one).await;
    stream_until_complete(&mut session, &harness, &prompt_two).await;

    let connection = server.single_connection();
    assert_eq!(connection.len(), 2);
    let second = connection.get(1).expect("missing request").body_json();

    assert_eq!(second["type"].as_str(), Some("response.create"));
    assert_eq!(second.get("previous_response_id"), None);
    assert_eq!(
        second["input"],
        serde_json::to_value(&prompt_two.input).expect("serialize full input")
    );

    server.shutdown().await;
}

use super::*;
use futures::SinkExt;
use futures::StreamExt;
use praxis_app_gateway_protocol::AccountUpdatedNotification;
use praxis_app_gateway_protocol::ConfigRequirementsReadResponse;
use praxis_app_gateway_protocol::GetAccountResponse;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::JSONRPCRequest;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::SessionSource as ApiSessionSource;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::ToolRequestUserInputParams;
use praxis_app_gateway_protocol::ToolRequestUserInputQuestion;
use praxis_core::config::ConfigBuilder;
use pretty_assertions::assert_eq;
use tokio::net::TcpListener;
use tokio::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::Request as WebSocketRequest;
use tokio_tungstenite::tungstenite::handshake::server::Response as WebSocketResponse;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

async fn build_test_config() -> Config {
    match ConfigBuilder::default().build().await {
        Ok(config) => config,
        Err(_) => {
            Config::load_default_with_cli_overrides(Vec::new()).expect("default config should load")
        }
    }
}

async fn start_test_client_with_capacity(
    session_source: SessionSource,
    channel_capacity: usize,
) -> NativeAppGatewayClient {
    NativeAppGatewayClient::start(NativeAppGatewayClientStartArgs {
        arg0_paths: Arg0DispatchPaths::default(),
        config: Arc::new(build_test_config().await),
        cli_overrides: Vec::new(),
        loader_overrides: LoaderOverrides::default(),
        cloud_requirements: CloudConfigBundleLoader::default(),
        feedback: PraxisFeedback::new(),
        config_warnings: Vec::new(),
        session_source,
        enable_praxis_api_key_env: false,
        client_name: "praxis-app-gateway-client-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity,
        control_listen: None,
        control_auth: NativeControlAuthSettings::default(),
    })
    .await
    .expect("in-process app-gateway client should start")
}

async fn start_test_client(session_source: SessionSource) -> NativeAppGatewayClient {
    start_test_client_with_capacity(session_source, DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY).await
}

async fn start_test_remote_server<F, Fut>(handler: F) -> String
where
    F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    start_test_remote_server_with_auth(/*expected_auth_token*/ None, handler).await
}

async fn start_test_remote_server_with_auth<F, Fut>(
    expected_auth_token: Option<String>,
    handler: F,
) -> String
where
    F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let websocket = accept_hdr_async(
            stream,
            move |request: &WebSocketRequest, response: WebSocketResponse| {
                let provided_auth_token = request
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let expected_auth_token = expected_auth_token
                    .as_ref()
                    .map(|token| format!("Bearer {token}"));
                assert_eq!(provided_auth_token, expected_auth_token);
                Ok(response)
            },
        )
        .await
        .expect("websocket upgrade should succeed");
        handler(websocket).await;
    });
    format!("ws://{addr}")
}

async fn expect_remote_initialize(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
    let JSONRPCMessage::Request(request) = read_websocket_message(websocket).await else {
        panic!("expected initialize request");
    };
    assert_eq!(request.method, "initialize");
    write_websocket_message(
        websocket,
        JSONRPCMessage::Response(JSONRPCResponse {
            id: request.id,
            result: serde_json::json!({}),
        }),
    )
    .await;

    let JSONRPCMessage::Notification(notification) = read_websocket_message(websocket).await else {
        panic!("expected initialized notification");
    };
    assert_eq!(notification.method, "initialized");
}

async fn read_websocket_message(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> JSONRPCMessage {
    loop {
        let frame = websocket
            .next()
            .await
            .expect("frame should be available")
            .expect("frame should decode");
        match frame {
            Message::Text(text) => {
                return serde_json::from_str::<JSONRPCMessage>(&text)
                    .expect("text frame should be valid JSON-RPC");
            }
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                continue;
            }
            Message::Close(_) => panic!("unexpected close frame"),
        }
    }
}

async fn write_websocket_message(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    message: JSONRPCMessage,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&message)
                .expect("message should serialize")
                .into(),
        ))
        .await
        .expect("message should send");
}

fn command_execution_output_delta_notification(delta: &str) -> ServerNotification {
    ServerNotification::CommandExecutionOutputDelta(
        praxis_app_gateway_protocol::CommandExecutionOutputDeltaNotification {
            thread_id: "thread".to_string(),
            turn_id: "turn".to_string(),
            item_id: "item".to_string(),
            delta: delta.to_string(),
        },
    )
}

fn agent_message_delta_notification(delta: &str) -> ServerNotification {
    ServerNotification::AgentMessageDelta(
        praxis_app_gateway_protocol::AgentMessageDeltaNotification {
            thread_id: "thread".to_string(),
            turn_id: "turn".to_string(),
            item_id: "item".to_string(),
            delta: delta.to_string(),
        },
    )
}

fn item_completed_notification(text: &str) -> ServerNotification {
    ServerNotification::ItemCompleted(praxis_app_gateway_protocol::ItemCompletedNotification {
        thread_id: "thread".to_string(),
        turn_id: "turn".to_string(),
        item: praxis_app_gateway_protocol::ThreadItem::AgentMessage {
            id: "item".to_string(),
            text: text.to_string(),
            phase: None,
            memory_citation: None,
        },
    })
}

fn turn_completed_notification() -> ServerNotification {
    ServerNotification::TurnCompleted(praxis_app_gateway_protocol::TurnCompletedNotification {
        thread_id: "thread".to_string(),
        turn: praxis_app_gateway_protocol::Turn {
            id: "turn".to_string(),
            items: Vec::new(),
            status: praxis_app_gateway_protocol::TurnStatus::Completed,
            error: None,
        },
    })
}

fn test_remote_connect_args(websocket_url: String) -> RemoteAppGatewayConnectArgs {
    RemoteAppGatewayConnectArgs {
        websocket_url,
        auth_token: None,
        client_name: "praxis-app-gateway-client-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: 8,
    }
}

#[tokio::test]
async fn typed_request_roundtrip_works() {
    let client = start_test_client(SessionSource::Exec).await;
    let _response: ConfigRequirementsReadResponse = client
        .request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(1),
            params: None,
        })
        .await
        .expect("typed request should succeed");
    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn typed_request_reports_json_rpc_errors() {
    let client = start_test_client(SessionSource::Exec).await;
    let err = client
        .request_typed::<ConfigRequirementsReadResponse>(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(99),
            params: praxis_app_gateway_protocol::ThreadReadParams {
                thread_id: "missing-thread".to_string(),
                include_turns: false,
                turn_limit: None,
            },
        })
        .await
        .expect_err("missing thread should return a JSON-RPC error");
    assert!(
        err.to_string().starts_with("thread/read failed:"),
        "expected method-qualified JSON-RPC failure message"
    );
    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn caller_provided_session_source_is_applied() {
    for (session_source, expected_source) in [
        (SessionSource::Exec, ApiSessionSource::Exec),
        (SessionSource::Cli, ApiSessionSource::Cli),
    ] {
        let client = start_test_client(session_source).await;
        let parsed: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("thread/start should succeed");
        assert_eq!(parsed.thread.source, expected_source);
        client.shutdown().await.expect("shutdown should complete");
    }
}

#[tokio::test]
async fn threads_started_via_app_gateway_are_visible_through_typed_requests() {
    let client = start_test_client(SessionSource::Cli).await;

    let response: ThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
            params: ThreadStartParams {
                ephemeral: Some(true),
                ..ThreadStartParams::default()
            },
        })
        .await
        .expect("thread/start should succeed");
    let read = client
        .request_typed::<praxis_app_gateway_protocol::ThreadReadResponse>(
            ClientRequest::ThreadRead {
                request_id: RequestId::Integer(4),
                params: praxis_app_gateway_protocol::ThreadReadParams {
                    thread_id: response.thread.id.clone(),
                    include_turns: false,
                    turn_limit: None,
                },
            },
        )
        .await
        .expect("thread/read should return the newly started thread");
    assert_eq!(read.thread.id, response.thread.id);

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn tiny_channel_capacity_still_supports_request_roundtrip() {
    let client = start_test_client_with_capacity(SessionSource::Exec, /*channel_capacity*/ 1).await;
    let _response: ConfigRequirementsReadResponse = client
        .request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(1),
            params: None,
        })
        .await
        .expect("typed request should succeed");
    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn forward_in_process_event_preserves_transcript_notifications_under_backpressure() {
    let (event_tx, mut event_rx) = mpsc::channel(1);
    event_tx
        .send(NativeGatewayEvent::Notification(
            command_execution_output_delta_notification("stdout-1"),
        ))
        .await
        .expect("initial event should enqueue");

    let mut skipped_events = 0usize;
    let result = forward_in_process_event(
        &event_tx,
        &mut skipped_events,
        NativeGatewayEvent::Notification(command_execution_output_delta_notification("stdout-2")),
        |_| {},
    )
    .await;
    assert_eq!(result, ForwardEventResult::Continue);
    assert_eq!(skipped_events, 1);

    let receive_task = tokio::spawn(async move {
        let mut events = Vec::new();
        for _ in 0..5 {
            events.push(
                timeout(Duration::from_secs(2), event_rx.recv())
                    .await
                    .expect("event should arrive before timeout")
                    .expect("event stream should stay open"),
            );
        }
        events
    });

    for notification in [
        agent_message_delta_notification("hello"),
        item_completed_notification("hello"),
        turn_completed_notification(),
    ] {
        let result = forward_in_process_event(
            &event_tx,
            &mut skipped_events,
            NativeGatewayEvent::Notification(notification),
            |_| {},
        )
        .await;
        assert_eq!(result, ForwardEventResult::Continue);
    }
    assert_eq!(skipped_events, 0);

    let events = receive_task
        .await
        .expect("receiver task should join successfully");
    assert!(matches!(
        &events[0],
        NativeGatewayEvent::Notification(
            ServerNotification::CommandExecutionOutputDelta(notification)
        ) if notification.delta == "stdout-1"
    ));
    assert!(matches!(
        &events[1],
        NativeGatewayEvent::Lagged { skipped: 1 }
    ));
    assert!(matches!(
        &events[2],
        NativeGatewayEvent::Notification(ServerNotification::AgentMessageDelta(
            notification
        )) if notification.delta == "hello"
    ));
    assert!(matches!(
        &events[3],
        NativeGatewayEvent::Notification(ServerNotification::ItemCompleted(
            notification
        )) if matches!(
            &notification.item,
            praxis_app_gateway_protocol::ThreadItem::AgentMessage { text, .. } if text == "hello"
        )
    ));
    assert!(matches!(
        &events[4],
        NativeGatewayEvent::Notification(ServerNotification::TurnCompleted(
            notification
        )) if notification.turn.status == praxis_app_gateway_protocol::TurnStatus::Completed
    ));
}

#[tokio::test]
async fn remote_typed_request_roundtrip_works() {
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        expect_remote_initialize(&mut websocket).await;
        let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
            panic!("expected account/read request");
        };
        assert_eq!(request.method, "account/read");
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::to_value(GetAccountResponse {
                    account: None,
                    requires_openai_auth: false,
                })
                .expect("response should serialize"),
            }),
        )
        .await;
        websocket.close(None).await.expect("close should succeed");
    })
    .await;
    let client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
        .await
        .expect("remote client should connect");

    let response: GetAccountResponse = client
        .request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: praxis_app_gateway_protocol::GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("typed request should succeed");
    assert_eq!(response.account, None);

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_connect_includes_auth_header_when_configured() {
    let auth_token = "remote-bearer-token".to_string();
    let websocket_url =
        start_test_remote_server_with_auth(Some(auth_token.clone()), |mut websocket| async move {
            expect_remote_initialize(&mut websocket).await;
            websocket.close(None).await.expect("close should succeed");
        })
        .await;
    let client = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
        auth_token: Some(auth_token),
        ..test_remote_connect_args(websocket_url)
    })
    .await
    .expect("remote client should connect");

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_connect_rejects_non_loopback_ws_when_auth_configured() {
    let result = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
        websocket_url: "ws://example.com:4500".to_string(),
        auth_token: Some("remote-bearer-token".to_string()),
        ..test_remote_connect_args("ws://127.0.0.1:1".to_string())
    })
    .await;
    let err = match result {
        Ok(_) => panic!("non-loopback ws should be rejected before connect"),
        Err(err) => err,
    };
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.to_string()
            .contains("remote auth tokens require `wss://` or loopback `ws://` URLs")
    );
}

#[test]
fn remote_auth_token_transport_policy_allows_wss_and_loopback_ws() {
    assert!(crate::remote::websocket_url_supports_auth_token(
        &url::Url::parse("wss://example.com:443").expect("wss URL should parse")
    ));
    assert!(crate::remote::websocket_url_supports_auth_token(
        &url::Url::parse("ws://127.0.0.1:4500").expect("loopback ws URL should parse")
    ));
    assert!(!crate::remote::websocket_url_supports_auth_token(
        &url::Url::parse("ws://example.com:4500").expect("non-loopback ws URL should parse")
    ));
}

#[tokio::test]
async fn remote_duplicate_request_id_keeps_original_waiter() {
    let (first_request_seen_tx, first_request_seen_rx) = tokio::sync::oneshot::channel();
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        expect_remote_initialize(&mut websocket).await;
        let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
            panic!("expected account/read request");
        };
        assert_eq!(request.method, "account/read");
        first_request_seen_tx
            .send(request.id.clone())
            .expect("request id should send");
        assert!(
            timeout(
                Duration::from_millis(100),
                read_websocket_message(&mut websocket)
            )
            .await
            .is_err(),
            "duplicate request should not be forwarded to the server"
        );
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::to_value(GetAccountResponse {
                    account: None,
                    requires_openai_auth: false,
                })
                .expect("response should serialize"),
            }),
        )
        .await;
        let _ = websocket.next().await;
    })
    .await;
    let client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
        .await
        .expect("remote client should connect");
    let first_request_handle = client.request_handle();
    let second_request_handle = first_request_handle.clone();

    let first_request = tokio::spawn(async move {
        first_request_handle
            .request_typed::<GetAccountResponse>(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: praxis_app_gateway_protocol::GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
    });

    let first_request_id = first_request_seen_rx
        .await
        .expect("server should observe the first request");
    assert_eq!(first_request_id, RequestId::Integer(1));

    let second_err = second_request_handle
        .request_typed::<GetAccountResponse>(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: praxis_app_gateway_protocol::GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect_err("duplicate request id should be rejected");
    assert_eq!(
        second_err.to_string(),
        "account/read transport error: duplicate remote app-gateway request id `1`"
    );

    let first_response = first_request
        .await
        .expect("first request task should join")
        .expect("first request should succeed");
    assert_eq!(
        first_response,
        GetAccountResponse {
            account: None,
            requires_openai_auth: false,
        }
    );

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_notifications_arrive_over_websocket() {
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        expect_remote_initialize(&mut websocket).await;
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Notification(
                serde_json::from_value(
                    serde_json::to_value(ServerNotification::AccountUpdated(
                        AccountUpdatedNotification {
                            auth_mode: None,
                            plan_type: None,
                        },
                    ))
                    .expect("notification should serialize"),
                )
                .expect("notification should convert to JSON-RPC"),
            ),
        )
        .await;
    })
    .await;
    let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
        .await
        .expect("remote client should connect");

    let event = client.next_event().await.expect("event should arrive");
    assert!(matches!(
        event,
        AppGatewayEvent::ServerNotification(ServerNotification::AccountUpdated(_))
    ));

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_backpressure_preserves_transcript_notifications() {
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        expect_remote_initialize(&mut websocket).await;
        for notification in [
            command_execution_output_delta_notification("stdout-1"),
            command_execution_output_delta_notification("stdout-2"),
            agent_message_delta_notification("hello"),
            item_completed_notification("hello"),
            turn_completed_notification(),
        ] {
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Notification(
                    serde_json::from_value(
                        serde_json::to_value(notification).expect("notification should serialize"),
                    )
                    .expect("notification should convert to JSON-RPC"),
                ),
            )
            .await;
        }
        let _ = done_rx.await;
    })
    .await;
    let mut client = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
        websocket_url,
        auth_token: None,
        client_name: "praxis-app-gateway-client-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: 1,
    })
    .await
    .expect("remote client should connect");

    let first_event = timeout(Duration::from_secs(2), client.next_event())
        .await
        .expect("first event should arrive before timeout")
        .expect("event stream should stay open");
    assert!(matches!(
        first_event,
        AppGatewayEvent::ServerNotification(ServerNotification::CommandExecutionOutputDelta(
            notification
        )) if notification.delta == "stdout-1"
    ));

    let mut remaining_events = Vec::new();
    for _ in 0..4 {
        remaining_events.push(
            timeout(Duration::from_secs(2), client.next_event())
                .await
                .expect("event should arrive before timeout")
                .expect("event stream should stay open"),
        );
    }

    let mut transcript_event_names = Vec::new();
    for event in &remaining_events {
        match event {
            AppGatewayEvent::Lagged { skipped: 1 } => {}
            AppGatewayEvent::ServerNotification(
                ServerNotification::CommandExecutionOutputDelta(notification),
            ) if notification.delta == "stdout-2" => {}
            AppGatewayEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                notification,
            )) if notification.delta == "hello" => {
                transcript_event_names.push("agent_message_delta");
            }
            AppGatewayEvent::ServerNotification(ServerNotification::ItemCompleted(
                notification,
            )) if matches!(
                &notification.item,
                praxis_app_gateway_protocol::ThreadItem::AgentMessage { text, .. } if text == "hello"
            ) =>
            {
                transcript_event_names.push("item_completed");
            }
            AppGatewayEvent::ServerNotification(ServerNotification::TurnCompleted(
                notification,
            )) if notification.turn.status
                == praxis_app_gateway_protocol::TurnStatus::Completed =>
            {
                transcript_event_names.push("turn_completed");
            }
            _ => panic!("unexpected remaining event: {event:?}"),
        }
    }
    assert_eq!(
        transcript_event_names,
        vec!["agent_message_delta", "item_completed", "turn_completed"]
    );

    done_tx
        .send(())
        .expect("server completion signal should send");
    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_server_request_resolution_roundtrip_works() {
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        expect_remote_initialize(&mut websocket).await;
        let request_id = RequestId::String("srv-1".to_string());
        let server_request = JSONRPCRequest {
            id: request_id.clone(),
            method: "item/tool/requestUserInput".to_string(),
            params: Some(
                serde_json::to_value(ToolRequestUserInputParams {
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    item_id: "call-1".to_string(),
                    questions: vec![ToolRequestUserInputQuestion {
                        id: "question-1".to_string(),
                        header: "Mode".to_string(),
                        question: "Pick one".to_string(),
                        is_other: false,
                        is_secret: false,
                        options: Some(vec![]),
                    }],
                })
                .expect("params should serialize"),
            ),
            trace: None,
        };
        write_websocket_message(&mut websocket, JSONRPCMessage::Request(server_request)).await;

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected server request response");
        };
        assert_eq!(response.id, request_id);
    })
    .await;
    let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
        .await
        .expect("remote client should connect");

    let AppGatewayEvent::ServerRequest(request) = client
        .next_event()
        .await
        .expect("request event should arrive")
    else {
        panic!("expected server request event");
    };
    client
        .resolve_server_request(request.id().clone(), serde_json::json!({}))
        .await
        .expect("server request should resolve");

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_server_request_received_during_initialize_is_delivered() {
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");

        let request_id = RequestId::String("srv-init".to_string());
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Request(JSONRPCRequest {
                id: request_id.clone(),
                method: "item/tool/requestUserInput".to_string(),
                params: Some(
                    serde_json::to_value(ToolRequestUserInputParams {
                        thread_id: "thread-1".to_string(),
                        turn_id: "turn-1".to_string(),
                        item_id: "call-1".to_string(),
                        questions: vec![ToolRequestUserInputQuestion {
                            id: "question-1".to_string(),
                            header: "Mode".to_string(),
                            question: "Pick one".to_string(),
                            is_other: false,
                            is_secret: false,
                            options: Some(vec![]),
                        }],
                    })
                    .expect("params should serialize"),
                ),
                trace: None,
            }),
        )
        .await;
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({}),
            }),
        )
        .await;

        let JSONRPCMessage::Notification(notification) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected server request response");
        };
        assert_eq!(response.id, request_id);
    })
    .await;
    let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
        .await
        .expect("remote client should connect");

    let AppGatewayEvent::ServerRequest(request) = client
        .next_event()
        .await
        .expect("request event should arrive")
    else {
        panic!("expected server request event");
    };
    client
        .resolve_server_request(request.id().clone(), serde_json::json!({}))
        .await
        .expect("server request should resolve");

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_unknown_server_request_is_rejected() {
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        expect_remote_initialize(&mut websocket).await;
        let request_id = RequestId::String("srv-unknown".to_string());
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Request(JSONRPCRequest {
                id: request_id.clone(),
                method: "thread/unknown".to_string(),
                params: None,
                trace: None,
            }),
        )
        .await;

        let JSONRPCMessage::Error(response) = read_websocket_message(&mut websocket).await else {
            panic!("expected JSON-RPC error response");
        };
        assert_eq!(response.id, request_id);
        assert_eq!(response.error.code, -32601);
        assert_eq!(
            response.error.message,
            "unsupported remote app-gateway request `thread/unknown`"
        );
    })
    .await;
    let client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
        .await
        .expect("remote client should connect");

    client.shutdown().await.expect("shutdown should complete");
}

#[tokio::test]
async fn remote_disconnect_surfaces_as_event() {
    let websocket_url = start_test_remote_server(|mut websocket| async move {
        expect_remote_initialize(&mut websocket).await;
        websocket.close(None).await.expect("close should succeed");
    })
    .await;
    let mut client = RemoteAppGatewayClient::connect(test_remote_connect_args(websocket_url))
        .await
        .expect("remote client should connect");

    let event = client
        .next_event()
        .await
        .expect("disconnect event should arrive");
    assert!(matches!(event, AppGatewayEvent::Disconnected { .. }));
}

#[test]
fn typed_request_error_exposes_sources() {
    let transport = TypedRequestError::Transport {
        method: "config/read".to_string(),
        source: IoError::new(ErrorKind::BrokenPipe, "closed"),
    };
    assert_eq!(std::error::Error::source(&transport).is_some(), true);

    let server = TypedRequestError::Server {
        method: "thread/read".to_string(),
        source: JSONRPCErrorError {
            code: -32603,
            data: None,
            message: "internal".to_string(),
        },
    };
    assert_eq!(std::error::Error::source(&server).is_some(), false);

    let deserialize = TypedRequestError::Deserialize {
        method: "thread/start".to_string(),
        source: serde_json::from_str::<u32>("\"nope\"")
            .expect_err("invalid integer should return deserialize error"),
    };
    assert_eq!(std::error::Error::source(&deserialize).is_some(), true);
}

#[tokio::test]
async fn next_event_surfaces_lagged_markers() {
    let (command_tx, _command_rx) = mpsc::channel(1);
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_handle = tokio::spawn(async {});
    event_tx
        .send(NativeGatewayEvent::Lagged { skipped: 3 })
        .await
        .expect("lagged marker should enqueue");
    drop(event_tx);

    let mut client = NativeAppGatewayClient {
        command_tx,
        event_rx,
        worker_handle,
    };

    let event = timeout(Duration::from_secs(2), client.next_event())
        .await
        .expect("lagged marker should arrive before timeout");
    assert!(matches!(
        event,
        Some(NativeGatewayEvent::Lagged { skipped: 3 })
    ));

    client.shutdown().await.expect("shutdown should complete");
}

#[test]
fn event_requires_delivery_marks_transcript_and_terminal_events() {
    assert!(event_requires_delivery(&NativeGatewayEvent::Notification(
        praxis_app_gateway_protocol::ServerNotification::TurnCompleted(
            praxis_app_gateway_protocol::TurnCompletedNotification {
                thread_id: "thread".to_string(),
                turn: praxis_app_gateway_protocol::Turn {
                    id: "turn".to_string(),
                    items: Vec::new(),
                    status: praxis_app_gateway_protocol::TurnStatus::Completed,
                    error: None,
                },
            }
        )
    )));
    assert!(event_requires_delivery(&NativeGatewayEvent::Notification(
        praxis_app_gateway_protocol::ServerNotification::AgentMessageDelta(
            praxis_app_gateway_protocol::AgentMessageDeltaNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: "hello".to_string(),
            }
        )
    )));
    assert!(event_requires_delivery(&NativeGatewayEvent::Notification(
        praxis_app_gateway_protocol::ServerNotification::ItemCompleted(
            praxis_app_gateway_protocol::ItemCompletedNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item: praxis_app_gateway_protocol::ThreadItem::AgentMessage {
                    id: "item".to_string(),
                    text: "hello".to_string(),
                    phase: None,
                    memory_citation: None,
                },
            }
        )
    )));
    assert!(!event_requires_delivery(&NativeGatewayEvent::Lagged {
        skipped: 1
    }));
    assert!(!event_requires_delivery(&NativeGatewayEvent::Notification(
        praxis_app_gateway_protocol::ServerNotification::CommandExecutionOutputDelta(
            praxis_app_gateway_protocol::CommandExecutionOutputDeltaNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: "stdout".to_string(),
            }
        )
    )));
}

#[tokio::test]
async fn runtime_start_args_leave_manager_bootstrap_to_app_gateway() {
    let config = Arc::new(build_test_config().await);

    let runtime_args = NativeAppGatewayClientStartArgs {
        arg0_paths: Arg0DispatchPaths::default(),
        config: config.clone(),
        cli_overrides: Vec::new(),
        loader_overrides: LoaderOverrides::default(),
        cloud_requirements: CloudConfigBundleLoader::default(),
        feedback: PraxisFeedback::new(),
        config_warnings: Vec::new(),
        session_source: SessionSource::Exec,
        enable_praxis_api_key_env: false,
        client_name: "praxis-app-gateway-client-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY,
        control_listen: None,
        control_auth: NativeControlAuthSettings::default(),
    }
    .into_runtime_start_args();

    assert_eq!(runtime_args.config, config);
}

#[tokio::test]
async fn shutdown_completes_promptly_without_retained_managers() {
    let client = start_test_client(SessionSource::Cli).await;

    timeout(Duration::from_secs(1), client.shutdown())
        .await
        .expect("shutdown should not wait for the 5s fallback timeout")
        .expect("shutdown should complete");
}

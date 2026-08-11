use std::time::Duration;

use praxis_app_gateway_protocol::AccountLoginCompletedNotification;
use praxis_app_gateway_protocol::AccountRateLimitsUpdatedNotification;
use praxis_app_gateway_protocol::AccountUpdatedNotification;
use praxis_app_gateway_protocol::AuthMode;
use praxis_app_gateway_protocol::ConfigWarningNotification;
use praxis_app_gateway_protocol::DynamicToolCallParams;
use praxis_app_gateway_protocol::FileChangeRequestApprovalParams;
use praxis_app_gateway_protocol::ModelRerouteReason;
use praxis_app_gateway_protocol::ModelReroutedNotification;
use praxis_app_gateway_protocol::RateLimitSnapshot;
use praxis_app_gateway_protocol::RateLimitWindow;
use praxis_app_gateway_protocol::ToolRequestUserInputParams;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::sync::Arc;
use tokio::time::timeout;
use uuid::Uuid;

use super::*;
use praxis_protocol::account::PlanType;

#[test]
fn verify_server_notification_serialization() {
    let notification =
        ServerNotification::AccountLoginCompleted(AccountLoginCompletedNotification {
            login_id: Some(Uuid::nil().to_string()),
            success: true,
            error: None,
        });

    let jsonrpc_notification = OutgoingMessage::AppGatewayNotification(notification);
    assert_eq!(
        json!({
            "method": "account/login/completed",
            "params": {
                "loginId": Uuid::nil().to_string(),
                "success": true,
                "error": null,
            },
        }),
        serde_json::to_value(jsonrpc_notification)
            .expect("ensure the strum macros serialize the method field correctly"),
        "ensure the strum macros serialize the method field correctly"
    );
}

#[test]
fn verify_account_login_completed_notification_serialization() {
    let notification =
        ServerNotification::AccountLoginCompleted(AccountLoginCompletedNotification {
            login_id: Some(Uuid::nil().to_string()),
            success: true,
            error: None,
        });

    let jsonrpc_notification = OutgoingMessage::AppGatewayNotification(notification);
    assert_eq!(
        json!({
            "method": "account/login/completed",
            "params": {
                "loginId": Uuid::nil().to_string(),
                "success": true,
                "error": null,
            },
        }),
        serde_json::to_value(jsonrpc_notification)
            .expect("ensure the notification serializes correctly"),
        "ensure the notification serializes correctly"
    );
}

#[test]
fn verify_account_rate_limits_notification_serialization() {
    let notification =
        ServerNotification::AccountRateLimitsUpdated(AccountRateLimitsUpdatedNotification {
            rate_limits: RateLimitSnapshot {
                limit_id: Some(OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID.to_string()),
                limit_name: None,
                primary: Some(RateLimitWindow {
                    used_percent: 25,
                    window_duration_mins: Some(15),
                    resets_at: Some(123),
                }),
                secondary: None,
                credits: None,
                plan_type: Some(PlanType::Plus),
            },
        });

    let jsonrpc_notification = OutgoingMessage::AppGatewayNotification(notification);
    assert_eq!(
        json!({
            "method": "account/rateLimits/updated",
            "params": {
                    "rateLimits": {
                    "limitId": OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID,
                    "limitName": null,
                    "primary": {
                        "usedPercent": 25,
                        "windowDurationMins": 15,
                        "resetsAt": 123
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": "plus"
                }
            },
        }),
        serde_json::to_value(jsonrpc_notification)
            .expect("ensure the notification serializes correctly"),
        "ensure the notification serializes correctly"
    );
}

#[test]
fn verify_account_updated_notification_serialization() {
    let notification = ServerNotification::AccountUpdated(AccountUpdatedNotification {
        auth_mode: Some(AuthMode::ApiKey),
        plan_type: None,
    });

    let jsonrpc_notification = OutgoingMessage::AppGatewayNotification(notification);
    assert_eq!(
        json!({
            "method": "account/updated",
            "params": {
                "authMode": "apikey",
                "planType": null
            },
        }),
        serde_json::to_value(jsonrpc_notification)
            .expect("ensure the notification serializes correctly"),
        "ensure the notification serializes correctly"
    );
}

#[test]
fn verify_config_warning_notification_serialization() {
    let notification = ServerNotification::ConfigWarning(ConfigWarningNotification {
        summary: "Config error: using defaults".to_string(),
        details: Some("error loading config: bad config".to_string()),
        path: None,
        range: None,
    });

    let jsonrpc_notification = OutgoingMessage::AppGatewayNotification(notification);
    assert_eq!(
        json!( {
            "method": "configWarning",
            "params": {
                "summary": "Config error: using defaults",
                "details": "error loading config: bad config",
            },
        }),
        serde_json::to_value(jsonrpc_notification)
            .expect("ensure the notification serializes correctly"),
        "ensure the notification serializes correctly"
    );
}

#[test]
fn verify_model_rerouted_notification_serialization() {
    let notification = ServerNotification::ModelRerouted(ModelReroutedNotification {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        from_model: "gpt-5.3-codex".to_string(),
        to_model: "gpt-5.2".to_string(),
        reason: ModelRerouteReason::HighRiskCyberActivity,
    });

    let jsonrpc_notification = OutgoingMessage::AppGatewayNotification(notification);
    assert_eq!(
        json!({
            "method": "model/rerouted",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "fromModel": "gpt-5.3-codex",
                "toModel": "gpt-5.2",
                "reason": "highRiskCyberActivity",
            },
        }),
        serde_json::to_value(jsonrpc_notification)
            .expect("ensure the notification serializes correctly"),
        "ensure the notification serializes correctly"
    );
}

#[tokio::test]
async fn send_response_routes_to_target_connection() {
    let (tx, mut rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = OutgoingMessageSender::new(tx);
    let request_id = ConnectionRequestId {
        connection_id: ConnectionId(42),
        request_id: RequestId::Integer(7),
    };

    outgoing
        .send_response(request_id.clone(), json!({ "ok": true }))
        .await;

    let envelope = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive envelope before timeout")
        .expect("channel should contain one message");

    match envelope {
        OutgoingEnvelope::ToConnection {
            connection_id,
            message,
            ..
        } => {
            assert_eq!(connection_id, ConnectionId(42));
            let OutgoingMessage::Response(response) = message else {
                panic!("expected response message");
            };
            assert_eq!(response.id, request_id.request_id);
            assert_eq!(response.result, json!({ "ok": true }));
        }
        other => panic!("expected targeted response envelope, got: {other:?}"),
    }
}

#[tokio::test]
async fn send_response_clears_registered_request_context() {
    let (tx, _rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = OutgoingMessageSender::new(tx);
    let request_id = ConnectionRequestId {
        connection_id: ConnectionId(42),
        request_id: RequestId::Integer(7),
    };

    outgoing
        .register_request_context(RequestContext::new(
            request_id.clone(),
            tracing::info_span!("app_gateway.request", rpc.method = "thread/start"),
            /*parent_trace*/ None,
        ))
        .await;
    assert_eq!(outgoing.request_context_count().await, 1);

    outgoing
        .send_response(request_id, json!({ "ok": true }))
        .await;

    assert_eq!(outgoing.request_context_count().await, 0);
}

#[tokio::test]
async fn send_error_routes_to_target_connection() {
    let (tx, mut rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = OutgoingMessageSender::new(tx);
    let request_id = ConnectionRequestId {
        connection_id: ConnectionId(9),
        request_id: RequestId::Integer(3),
    };
    let error = JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: "boom".to_string(),
        data: None,
    };

    outgoing.send_error(request_id.clone(), error.clone()).await;

    let envelope = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive envelope before timeout")
        .expect("channel should contain one message");

    match envelope {
        OutgoingEnvelope::ToConnection {
            connection_id,
            message,
            ..
        } => {
            assert_eq!(connection_id, ConnectionId(9));
            let OutgoingMessage::Error(outgoing_error) = message else {
                panic!("expected error message");
            };
            assert_eq!(outgoing_error.id, RequestId::Integer(3));
            assert_eq!(outgoing_error.error, error);
        }
        other => panic!("expected targeted error envelope, got: {other:?}"),
    }
}

#[tokio::test]
async fn send_server_notification_to_connection_and_wait_tracks_write_completion() {
    let (tx, mut rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = OutgoingMessageSender::new(tx);
    let send_task = tokio::spawn(async move {
        outgoing
            .send_server_notification_to_connection_and_wait(
                ConnectionId(42),
                ServerNotification::ModelRerouted(ModelReroutedNotification {
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    from_model: "gpt-5.3-codex".to_string(),
                    to_model: "gpt-5.2".to_string(),
                    reason: ModelRerouteReason::HighRiskCyberActivity,
                }),
            )
            .await
    });

    let envelope = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive envelope before timeout")
        .expect("channel should contain one message");
    let OutgoingEnvelope::ToConnection {
        connection_id,
        message,
        write_complete_tx,
    } = envelope
    else {
        panic!("expected targeted server notification envelope");
    };
    assert_eq!(connection_id, ConnectionId(42));
    assert!(matches!(
        message,
        OutgoingMessage::AppGatewayNotification(_)
    ));
    write_complete_tx
        .expect("write completion sender should be attached")
        .send(())
        .expect("receiver should still be waiting");

    timeout(Duration::from_secs(1), send_task)
        .await
        .expect("send task should finish after write completion is signaled")
        .expect("send task should not panic");
}

#[tokio::test]
async fn connection_closed_clears_registered_request_contexts() {
    let (tx, _rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = OutgoingMessageSender::new(tx);
    let closed_connection_request = ConnectionRequestId {
        connection_id: ConnectionId(9),
        request_id: RequestId::Integer(3),
    };
    let open_connection_request = ConnectionRequestId {
        connection_id: ConnectionId(10),
        request_id: RequestId::Integer(4),
    };

    outgoing
        .register_request_context(RequestContext::new(
            closed_connection_request,
            tracing::info_span!("app_gateway.request", rpc.method = "turn/interrupt"),
            /*parent_trace*/ None,
        ))
        .await;
    outgoing
        .register_request_context(RequestContext::new(
            open_connection_request,
            tracing::info_span!("app_gateway.request", rpc.method = "turn/start"),
            /*parent_trace*/ None,
        ))
        .await;
    assert_eq!(outgoing.request_context_count().await, 2);

    outgoing.connection_closed(ConnectionId(9)).await;

    assert_eq!(outgoing.request_context_count().await, 1);
}

#[tokio::test]
async fn notify_client_error_forwards_error_to_waiter() {
    let (tx, _rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = OutgoingMessageSender::new(tx);

    let (request_id, wait_for_result) = outgoing
        .send_request(ServerRequestPayload::FileChangeRequestApproval(
            FileChangeRequestApprovalParams {
                thread_id: ThreadId::new().to_string(),
                turn_id: "turn-id".to_string(),
                item_id: "call-id".to_string(),
                reason: None,
                grant_root: None,
            },
        ))
        .await;

    let error = JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: "refresh failed".to_string(),
        data: None,
    };

    outgoing
        .notify_client_error(ConnectionId(1), request_id, error.clone())
        .await;

    let result = timeout(Duration::from_secs(1), wait_for_result)
        .await
        .expect("wait should not time out")
        .expect("waiter should receive a callback");
    assert_eq!(result, Err(error));
}

#[tokio::test]
async fn response_from_other_connection_does_not_consume_waiter() {
    let (tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let controller_connection_id = ConnectionId(41);
    let other_connection_id = ConnectionId(42);
    let thread_outgoing = ThreadScopedOutgoingMessageSender::new(
        outgoing.clone(),
        vec![controller_connection_id],
        ThreadId::new(),
    );
    let (request_id, mut waiter) = thread_outgoing
        .send_request(ServerRequestPayload::FileChangeRequestApproval(
            FileChangeRequestApprovalParams {
                thread_id: ThreadId::new().to_string(),
                turn_id: "turn-id".to_string(),
                item_id: "call-id".to_string(),
                reason: None,
                grant_root: None,
            },
        ))
        .await;
    outgoing_rx.recv().await.expect("request should be sent");

    outgoing
        .notify_client_response(other_connection_id, request_id.clone(), json!({}))
        .await;
    assert!(
        timeout(Duration::from_millis(20), &mut waiter)
            .await
            .is_err(),
        "non-controlling response must leave the waiter pending"
    );

    outgoing
        .notify_client_response(
            controller_connection_id,
            request_id,
            json!({ "decision": "decline" }),
        )
        .await;
    assert_eq!(
        waiter.await.expect("waiter should receive owner response"),
        Ok(json!({ "decision": "decline" }))
    );
}

#[tokio::test]
async fn controlling_connection_close_fails_waiter_without_holding_registry_lock() {
    let (tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let controller_connection_id = ConnectionId(51);
    let thread_outgoing = ThreadScopedOutgoingMessageSender::new(
        outgoing.clone(),
        vec![controller_connection_id],
        ThreadId::new(),
    );
    let (_request_id, waiter) = thread_outgoing
        .send_request(ServerRequestPayload::FileChangeRequestApproval(
            FileChangeRequestApprovalParams {
                thread_id: ThreadId::new().to_string(),
                turn_id: "turn-id".to_string(),
                item_id: "call-id".to_string(),
                reason: None,
                grant_root: None,
            },
        ))
        .await;
    outgoing_rx.recv().await.expect("request should be sent");

    outgoing.connection_closed(controller_connection_id).await;

    let error = waiter
        .await
        .expect("disconnect must wake waiter")
        .expect_err("disconnect must fail closed");
    assert_eq!(
        error.data,
        Some(json!({ "reason": "controllerConnectionClosed" }))
    );
}

#[tokio::test]
async fn request_registered_after_connection_close_fails_immediately() {
    let (tx, _outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(4);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let controller_connection_id = ConnectionId(61);
    outgoing.connection_closed(controller_connection_id).await;
    let thread_outgoing = ThreadScopedOutgoingMessageSender::new(
        outgoing,
        vec![controller_connection_id],
        ThreadId::new(),
    );

    let (_request_id, waiter) = thread_outgoing
        .send_request(ServerRequestPayload::FileChangeRequestApproval(
            FileChangeRequestApprovalParams {
                thread_id: ThreadId::new().to_string(),
                turn_id: "turn-id".to_string(),
                item_id: "call-id".to_string(),
                reason: None,
                grant_root: None,
            },
        ))
        .await;

    let error = waiter
        .await
        .expect("closed controller must resolve waiter")
        .expect_err("closed controller must fail closed");
    assert_eq!(
        error.data,
        Some(json!({ "reason": "controllerConnectionClosed" }))
    );
}

#[tokio::test]
async fn pending_requests_for_thread_returns_thread_requests_in_request_id_order() {
    let (tx, _rx) = mpsc::channel::<OutgoingEnvelope>(8);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let thread_id = ThreadId::new();
    let thread_outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing.clone(), vec![ConnectionId(1)], thread_id);

    let (dynamic_tool_request_id, _dynamic_tool_waiter) = thread_outgoing
        .send_request(ServerRequestPayload::DynamicToolCall(
            DynamicToolCallParams {
                thread_id: thread_id.to_string(),
                turn_id: "turn-1".to_string(),
                call_id: "call-0".to_string(),
                tool: "tool".to_string(),
                arguments: json!({}),
            },
        ))
        .await;
    let (first_request_id, _first_waiter) = thread_outgoing
        .send_request(ServerRequestPayload::ToolRequestUserInput(
            ToolRequestUserInputParams {
                thread_id: thread_id.to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "call-1".to_string(),
                questions: vec![],
            },
        ))
        .await;
    let (second_request_id, _second_waiter) = thread_outgoing
        .send_request(ServerRequestPayload::FileChangeRequestApproval(
            FileChangeRequestApprovalParams {
                thread_id: thread_id.to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "call-2".to_string(),
                reason: None,
                grant_root: None,
            },
        ))
        .await;
    let pending_requests = outgoing.pending_requests_for_thread(thread_id).await;
    assert_eq!(
        pending_requests
            .iter()
            .map(ServerRequest::id)
            .collect::<Vec<_>>(),
        vec![
            &dynamic_tool_request_id,
            &first_request_id,
            &second_request_id
        ]
    );
}

#[tokio::test]
async fn cancel_requests_for_thread_cancels_all_thread_requests() {
    let (tx, _rx) = mpsc::channel::<OutgoingEnvelope>(8);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let thread_id = ThreadId::new();
    let thread_outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing.clone(), vec![ConnectionId(1)], thread_id);

    let (_dynamic_tool_request_id, dynamic_tool_waiter) = thread_outgoing
        .send_request(ServerRequestPayload::DynamicToolCall(
            DynamicToolCallParams {
                thread_id: thread_id.to_string(),
                turn_id: "turn-1".to_string(),
                call_id: "call-0".to_string(),
                tool: "tool".to_string(),
                arguments: json!({}),
            },
        ))
        .await;
    let (_request_id, user_input_waiter) = thread_outgoing
        .send_request(ServerRequestPayload::ToolRequestUserInput(
            ToolRequestUserInputParams {
                thread_id: thread_id.to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "call-1".to_string(),
                questions: vec![],
            },
        ))
        .await;
    let error = JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: "tracked request cancelled".to_string(),
        data: None,
    };

    outgoing
        .cancel_requests_for_thread(thread_id, Some(error.clone()))
        .await;

    let dynamic_tool_result = timeout(Duration::from_secs(1), dynamic_tool_waiter)
        .await
        .expect("dynamic tool waiter should resolve")
        .expect("dynamic tool waiter should receive a callback");
    let user_input_result = timeout(Duration::from_secs(1), user_input_waiter)
        .await
        .expect("user input waiter should resolve")
        .expect("user input waiter should receive a callback");
    assert_eq!(dynamic_tool_result, Err(error.clone()));
    assert_eq!(user_input_result, Err(error));
    assert!(
        outgoing
            .pending_requests_for_thread(thread_id)
            .await
            .is_empty()
    );
}

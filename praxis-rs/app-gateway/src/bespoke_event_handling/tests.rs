use super::*;
use crate::CHANNEL_CAPACITY;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use crate::outgoing_message::OutgoingMessageSender;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use praxis_app_gateway_protocol::CollabAgentState as ApiCollabAgentStatus;
use praxis_app_gateway_protocol::CollabAgentTool;
use praxis_app_gateway_protocol::CollabAgentToolCallStatus as ApiCollabToolCallStatus;
use praxis_app_gateway_protocol::FileChangeApprovalDecision;
use praxis_app_gateway_protocol::GuardianApprovalReviewStatus;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::TurnPlanStepStatus;
use praxis_protocol::items::HookPromptFragment;
use praxis_protocol::items::build_hook_prompt_message;
use praxis_protocol::models::FileSystemPermissions as CoreFileSystemPermissions;
use praxis_protocol::models::NetworkPermissions as CoreNetworkPermissions;
use praxis_protocol::plan_tool::PlanItemArg;
use praxis_protocol::plan_tool::StepStatus;
use praxis_protocol::protocol::CollabResumeBeginEvent;
use praxis_protocol::protocol::CollabResumeEndEvent;
use praxis_protocol::protocol::CreditsSnapshot;
use praxis_protocol::protocol::OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID;
use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_protocol::protocol::RateLimitWindow;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

fn new_thread_state() -> Arc<Mutex<ThreadState>> {
    Arc::new(Mutex::new(ThreadState::default()))
}

async fn recv_broadcast_message(
    rx: &mut mpsc::Receiver<OutgoingEnvelope>,
) -> Result<OutgoingMessage> {
    let envelope = rx
        .recv()
        .await
        .ok_or_else(|| anyhow!("should send one message"))?;
    match envelope {
        OutgoingEnvelope::Broadcast { message } => Ok(message),
        OutgoingEnvelope::ToConnection { message, .. } => Ok(message),
    }
}

#[test]
fn guardian_assessment_started_uses_event_turn_id_fallback() {
    let conversation_id = ThreadId::new();
    let action = praxis_protocol::protocol::GuardianAssessmentAction::Command {
        source: praxis_protocol::protocol::GuardianCommandSource::Shell,
        command: "rm -rf /tmp/example.sqlite".to_string(),
        cwd: "/tmp".into(),
    };
    let notification = guardian_auto_approval_review_notification(
        &conversation_id,
        "turn-from-event",
        &GuardianAssessmentEvent {
            id: "item-1".to_string(),
            turn_id: String::new(),
            status: praxis_protocol::protocol::GuardianAssessmentStatus::InProgress,
            risk_score: None,
            risk_level: None,
            rationale: None,
            action: action.clone(),
        },
    );

    match notification {
        ServerNotification::ItemGuardianApprovalReviewStarted(payload) => {
            assert_eq!(payload.thread_id, conversation_id.to_string());
            assert_eq!(payload.turn_id, "turn-from-event");
            assert_eq!(payload.target_item_id, "item-1");
            assert_eq!(
                payload.review.status,
                GuardianApprovalReviewStatus::InProgress
            );
            assert_eq!(payload.review.risk_score, None);
            assert_eq!(payload.review.risk_level, None);
            assert_eq!(payload.review.rationale, None);
            assert_eq!(payload.action, action.into());
        }
        other => panic!("unexpected notification: {other:?}"),
    }
}

#[test]
fn guardian_assessment_completed_emits_review_payload() {
    let conversation_id = ThreadId::new();
    let action = praxis_protocol::protocol::GuardianAssessmentAction::Command {
        source: praxis_protocol::protocol::GuardianCommandSource::Shell,
        command: "rm -rf /tmp/example.sqlite".to_string(),
        cwd: "/tmp".into(),
    };
    let notification = guardian_auto_approval_review_notification(
        &conversation_id,
        "turn-from-event",
        &GuardianAssessmentEvent {
            id: "item-2".to_string(),
            turn_id: "turn-from-assessment".to_string(),
            status: praxis_protocol::protocol::GuardianAssessmentStatus::Denied,
            risk_score: Some(91),
            risk_level: Some(praxis_protocol::protocol::GuardianRiskLevel::High),
            rationale: Some("too risky".to_string()),
            action: action.clone(),
        },
    );

    match notification {
        ServerNotification::ItemGuardianApprovalReviewCompleted(payload) => {
            assert_eq!(payload.thread_id, conversation_id.to_string());
            assert_eq!(payload.turn_id, "turn-from-assessment");
            assert_eq!(payload.target_item_id, "item-2");
            assert_eq!(payload.review.status, GuardianApprovalReviewStatus::Denied);
            assert_eq!(payload.review.risk_score, Some(91));
            assert_eq!(
                payload.review.risk_level,
                Some(praxis_app_gateway_protocol::GuardianRiskLevel::High)
            );
            assert_eq!(payload.review.rationale.as_deref(), Some("too risky"));
            assert_eq!(payload.action, action.into());
        }
        other => panic!("unexpected notification: {other:?}"),
    }
}

#[test]
fn guardian_assessment_aborted_emits_completed_review_payload() {
    let conversation_id = ThreadId::new();
    let action = praxis_protocol::protocol::GuardianAssessmentAction::NetworkAccess {
        target: "api.openai.com:443".to_string(),
        host: "api.openai.com".to_string(),
        protocol: praxis_protocol::protocol::NetworkApprovalProtocol::Https,
        port: 443,
    };
    let notification = guardian_auto_approval_review_notification(
        &conversation_id,
        "turn-from-event",
        &GuardianAssessmentEvent {
            id: "item-3".to_string(),
            turn_id: "turn-from-assessment".to_string(),
            status: praxis_protocol::protocol::GuardianAssessmentStatus::Aborted,
            risk_score: None,
            risk_level: None,
            rationale: None,
            action: action.clone(),
        },
    );

    match notification {
        ServerNotification::ItemGuardianApprovalReviewCompleted(payload) => {
            assert_eq!(payload.thread_id, conversation_id.to_string());
            assert_eq!(payload.turn_id, "turn-from-assessment");
            assert_eq!(payload.target_item_id, "item-3");
            assert_eq!(payload.review.status, GuardianApprovalReviewStatus::Aborted);
            assert_eq!(payload.review.risk_score, None);
            assert_eq!(payload.review.risk_level, None);
            assert_eq!(payload.review.rationale, None);
            assert_eq!(payload.action, action.into());
        }
        other => panic!("unexpected notification: {other:?}"),
    }
}

#[test]
fn file_change_accept_for_session_maps_to_approved_for_session() {
    let (decision, completion_status) =
        map_file_change_approval_decision(FileChangeApprovalDecision::AcceptForSession);
    assert_eq!(decision, ReviewDecision::ApprovedForSession);
    assert_eq!(completion_status, None);
}

#[test]
fn mcp_server_elicitation_turn_transition_error_maps_to_cancel() {
    let error = JSONRPCErrorError {
        code: -1,
        message: "client request resolved because the turn state was changed".to_string(),
        data: Some(serde_json::json!({ "reason": "turnTransition" })),
    };

    let response = mcp_server_elicitation_response_from_client_result(Ok(Err(error)));

    assert_eq!(
        response,
        McpServerElicitationRequestResponse {
            action: McpServerElicitationAction::Cancel,
            content: None,
            meta: None,
        }
    );
}

#[test]
fn request_permissions_turn_transition_error_is_ignored() {
    let error = JSONRPCErrorError {
        code: -1,
        message: "client request resolved because the turn state was changed".to_string(),
        data: Some(serde_json::json!({ "reason": "turnTransition" })),
    };

    let response = request_permissions_response_from_client_result(
        CoreRequestPermissionProfile::default(),
        Ok(Err(error)),
    );

    assert_eq!(response, None);
}

#[test]
fn request_permissions_response_accepts_partial_network_and_file_system_grants() {
    let input_path = if cfg!(target_os = "windows") {
        r"C:\tmp\input"
    } else {
        "/tmp/input"
    };
    let output_path = if cfg!(target_os = "windows") {
        r"C:\tmp\output"
    } else {
        "/tmp/output"
    };
    let ignored_path = if cfg!(target_os = "windows") {
        r"C:\tmp\ignored"
    } else {
        "/tmp/ignored"
    };
    let absolute_path = |path: &str| {
        AbsolutePathBuf::try_from(std::path::PathBuf::from(path)).expect("absolute path")
    };
    let requested_permissions = CoreRequestPermissionProfile {
        network: Some(CoreNetworkPermissions {
            enabled: Some(true),
        }),
        file_system: Some(CoreFileSystemPermissions {
            read: Some(vec![absolute_path(input_path)]),
            write: Some(vec![absolute_path(output_path)]),
        }),
    };
    let cases = vec![
        (
            serde_json::json!({}),
            CoreRequestPermissionProfile::default(),
        ),
        (
            serde_json::json!({
                "network": {
                    "enabled": true,
                },
            }),
            CoreRequestPermissionProfile {
                network: Some(CoreNetworkPermissions {
                    enabled: Some(true),
                }),
                ..CoreRequestPermissionProfile::default()
            },
        ),
        (
            serde_json::json!({
                "fileSystem": {
                    "write": [output_path],
                },
            }),
            CoreRequestPermissionProfile {
                file_system: Some(CoreFileSystemPermissions {
                    read: None,
                    write: Some(vec![absolute_path(output_path)]),
                }),
                ..CoreRequestPermissionProfile::default()
            },
        ),
        (
            serde_json::json!({
                "fileSystem": {
                    "read": [input_path],
                    "write": [output_path, ignored_path],
                },
                "macos": {
                    "calendar": true,
                },
            }),
            CoreRequestPermissionProfile {
                file_system: Some(CoreFileSystemPermissions {
                    read: Some(vec![absolute_path(input_path)]),
                    write: Some(vec![absolute_path(output_path)]),
                }),
                ..CoreRequestPermissionProfile::default()
            },
        ),
    ];

    for (granted_permissions, expected_permissions) in cases {
        let response = request_permissions_response_from_client_result(
            requested_permissions.clone(),
            Ok(Ok(serde_json::json!({
                "permissions": granted_permissions,
            }))),
        )
        .expect("response should be accepted");

        assert_eq!(
            response,
            CoreRequestPermissionsResponse {
                permissions: expected_permissions,
                scope: CorePermissionGrantScope::Turn,
            }
        );
    }
}

#[test]
fn request_permissions_response_preserves_session_scope() {
    let response = request_permissions_response_from_client_result(
        CoreRequestPermissionProfile::default(),
        Ok(Ok(serde_json::json!({
            "scope": "session",
            "permissions": {},
        }))),
    )
    .expect("response should be accepted");

    assert_eq!(
        response,
        CoreRequestPermissionsResponse {
            permissions: CoreRequestPermissionProfile::default(),
            scope: CorePermissionGrantScope::Session,
        }
    );
}

#[test]
fn collab_resume_begin_maps_to_item_started_resume_thread() {
    let event = CollabResumeBeginEvent {
        call_id: "call-1".to_string(),
        sender_thread_id: ThreadId::new(),
        receiver_thread_id: ThreadId::new(),
        receiver_agent_base_name: None,
        receiver_agent_title: None,
        receiver_agent_display_name: None,
        receiver_agent_role: None,
    };

    let item = collab_resume_begin_item(event.clone());
    let expected = ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::ResumeThread,
        status: ApiCollabToolCallStatus::InProgress,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![event.receiver_thread_id.to_string()],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states: HashMap::new(),
    };
    assert_eq!(item, expected);
}

#[test]
fn collab_resume_end_maps_to_item_completed_resume_thread() {
    let event = CollabResumeEndEvent {
        call_id: "call-2".to_string(),
        sender_thread_id: ThreadId::new(),
        receiver_thread_id: ThreadId::new(),
        receiver_agent_base_name: None,
        receiver_agent_title: None,
        receiver_agent_display_name: None,
        receiver_agent_role: None,
        status: praxis_protocol::protocol::AgentStatus::NotFound,
    };

    let item = collab_resume_end_item(event.clone());
    let receiver_id = event.receiver_thread_id.to_string();
    let expected = ThreadItem::CollabAgentToolCall {
        id: event.call_id,
        tool: CollabAgentTool::ResumeThread,
        status: ApiCollabToolCallStatus::Failed,
        sender_thread_id: event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![receiver_id.clone()],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states: [(
            receiver_id,
            ApiCollabAgentStatus::from(praxis_protocol::protocol::AgentStatus::NotFound),
        )]
        .into_iter()
        .collect(),
    };
    assert_eq!(item, expected);
}

#[tokio::test]
async fn test_handle_error_records_message() -> Result<()> {
    let conversation_id = ThreadId::new();
    let thread_state = new_thread_state();

    handle_error(
        conversation_id,
        TurnError {
            message: "boom".to_string(),
            praxis_error_info: Some(ApiPraxisErrorInfo::InternalServerError),
            additional_details: None,
        },
        &thread_state,
    )
    .await;

    let turn_summary = find_and_remove_turn_summary(conversation_id, &thread_state).await;
    assert_eq!(
        turn_summary.last_error,
        Some(TurnError {
            message: "boom".to_string(),
            praxis_error_info: Some(ApiPraxisErrorInfo::InternalServerError),
            additional_details: None,
        })
    );
    Ok(())
}

#[tokio::test]
async fn test_handle_turn_complete_emits_completed_without_error() -> Result<()> {
    let conversation_id = ThreadId::new();
    let event_turn_id = "complete1".to_string();
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());
    let thread_state = new_thread_state();

    handle_turn_complete(
        conversation_id,
        event_turn_id.clone(),
        &outgoing,
        &thread_state,
    )
    .await;

    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
            assert_eq!(n.turn.id, event_turn_id);
            assert_eq!(n.turn.status, TurnStatus::Completed);
            assert_eq!(n.turn.error, None);
        }
        other => bail!("unexpected message: {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "no extra messages expected");
    Ok(())
}

#[tokio::test]
async fn test_handle_turn_interrupted_emits_interrupted_with_error() -> Result<()> {
    let conversation_id = ThreadId::new();
    let event_turn_id = "interrupt1".to_string();
    let thread_state = new_thread_state();
    handle_error(
        conversation_id,
        TurnError {
            message: "oops".to_string(),
            praxis_error_info: None,
            additional_details: None,
        },
        &thread_state,
    )
    .await;
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());

    handle_turn_interrupted(
        conversation_id,
        event_turn_id.clone(),
        &outgoing,
        &thread_state,
    )
    .await;

    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
            assert_eq!(n.turn.id, event_turn_id);
            assert_eq!(n.turn.status, TurnStatus::Interrupted);
            assert_eq!(n.turn.error, None);
        }
        other => bail!("unexpected message: {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "no extra messages expected");
    Ok(())
}

#[tokio::test]
async fn test_handle_turn_complete_emits_failed_with_error() -> Result<()> {
    let conversation_id = ThreadId::new();
    let event_turn_id = "complete_err1".to_string();
    let thread_state = new_thread_state();
    handle_error(
        conversation_id,
        TurnError {
            message: "bad".to_string(),
            praxis_error_info: Some(ApiPraxisErrorInfo::Other),
            additional_details: None,
        },
        &thread_state,
    )
    .await;
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());

    handle_turn_complete(
        conversation_id,
        event_turn_id.clone(),
        &outgoing,
        &thread_state,
    )
    .await;

    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
            assert_eq!(n.turn.id, event_turn_id);
            assert_eq!(n.turn.status, TurnStatus::Failed);
            assert_eq!(
                n.turn.error,
                Some(TurnError {
                    message: "bad".to_string(),
                    praxis_error_info: Some(ApiPraxisErrorInfo::Other),
                    additional_details: None,
                })
            );
        }
        other => bail!("unexpected message: {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "no extra messages expected");
    Ok(())
}

#[tokio::test]
async fn test_handle_turn_plan_update_emits_notification() -> Result<()> {
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());
    let update = UpdatePlanArgs {
        explanation: Some("need plan".to_string()),
        plan: vec![
            PlanItemArg {
                step: "first".to_string(),
                status: StepStatus::Pending,
            },
            PlanItemArg {
                step: "second".to_string(),
                status: StepStatus::Completed,
            },
        ],
    };

    let conversation_id = ThreadId::new();

    handle_turn_plan_update(conversation_id, "turn-123", update, &outgoing).await;

    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnPlanUpdated(n)) => {
            assert_eq!(n.thread_id, conversation_id.to_string());
            assert_eq!(n.turn_id, "turn-123");
            assert_eq!(n.explanation.as_deref(), Some("need plan"));
            assert_eq!(n.plan.len(), 2);
            assert_eq!(n.plan[0].step, "first");
            assert_eq!(n.plan[0].status, TurnPlanStepStatus::Pending);
            assert_eq!(n.plan[1].step, "second");
            assert_eq!(n.plan[1].status, TurnPlanStepStatus::Completed);
        }
        other => bail!("unexpected message: {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "no extra messages expected");
    Ok(())
}

#[tokio::test]
async fn test_handle_token_count_event_emits_usage_and_rate_limits() -> Result<()> {
    let conversation_id = ThreadId::new();
    let turn_id = "turn-123".to_string();
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());

    let info = TokenUsageInfo {
        total_token_usage: TokenUsage {
            input_tokens: 100,
            cached_input_tokens: 25,
            cache_reported_input_tokens: 100,
            output_tokens: 50,
            reasoning_output_tokens: 9,
            total_tokens: 200,
        },
        last_token_usage: TokenUsage {
            input_tokens: 10,
            cached_input_tokens: 5,
            cache_reported_input_tokens: 10,
            output_tokens: 7,
            reasoning_output_tokens: 1,
            total_tokens: 23,
        },
        model_context_window: Some(4096),
        model_auto_compact_token_limit: Some(3600),
    };
    let rate_limits = RateLimitSnapshot {
        limit_id: Some(OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID.to_string()),
        limit_name: None,
        primary: Some(RateLimitWindow {
            used_percent: 42.5,
            window_minutes: Some(15),
            resets_at: Some(1700000000),
        }),
        secondary: None,
        credits: Some(CreditsSnapshot {
            has_credits: true,
            unlimited: false,
            balance: Some("5".to_string()),
        }),
        plan_type: None,
    };

    handle_token_count_event(
        conversation_id,
        turn_id.clone(),
        TokenCountEvent {
            info: Some(info),
            rate_limits: Some(rate_limits),
        },
        &outgoing,
    )
    .await;

    let first = recv_broadcast_message(&mut rx).await?;
    match first {
        OutgoingMessage::AppGatewayNotification(ServerNotification::ThreadTokenUsageUpdated(
            payload,
        )) => {
            assert_eq!(payload.thread_id, conversation_id.to_string());
            assert_eq!(payload.turn_id, turn_id);
            let usage = payload.token_usage;
            assert_eq!(usage.total.total_tokens, 200);
            assert_eq!(usage.total.cached_input_tokens, 25);
            assert_eq!(usage.total.cache_reported_input_tokens, 100);
            assert_eq!(usage.last.output_tokens, 7);
            assert_eq!(usage.model_context_window, Some(4096));
        }
        other => bail!("unexpected notification: {other:?}"),
    }

    let second = recv_broadcast_message(&mut rx).await?;
    match second {
        OutgoingMessage::AppGatewayNotification(ServerNotification::AccountRateLimitsUpdated(
            payload,
        )) => {
            assert_eq!(
                payload.rate_limits.limit_id.as_deref(),
                Some(OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID)
            );
            assert_eq!(payload.rate_limits.limit_name, None);
            assert!(payload.rate_limits.primary.is_some());
            assert!(payload.rate_limits.credits.is_some());
        }
        other => bail!("unexpected notification: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_handle_token_count_event_without_usage_info() -> Result<()> {
    let conversation_id = ThreadId::new();
    let turn_id = "turn-456".to_string();
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());

    handle_token_count_event(
        conversation_id,
        turn_id.clone(),
        TokenCountEvent {
            info: None,
            rate_limits: None,
        },
        &outgoing,
    )
    .await;

    assert!(
        rx.try_recv().is_err(),
        "no notifications should be emitted when token usage info is absent"
    );
    Ok(())
}

#[tokio::test]
async fn test_handle_turn_complete_emits_error_multiple_turns() -> Result<()> {
    // Conversation A will have two turns; Conversation B will have one turn.
    let conversation_a = ThreadId::new();
    let conversation_b = ThreadId::new();
    let thread_state = new_thread_state();

    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());

    // Turn 1 on conversation A
    let a_turn1 = "a_turn1".to_string();
    handle_error(
        conversation_a,
        TurnError {
            message: "a1".to_string(),
            praxis_error_info: Some(ApiPraxisErrorInfo::BadRequest),
            additional_details: None,
        },
        &thread_state,
    )
    .await;
    handle_turn_complete(conversation_a, a_turn1.clone(), &outgoing, &thread_state).await;

    // Turn 1 on conversation B
    let b_turn1 = "b_turn1".to_string();
    handle_error(
        conversation_b,
        TurnError {
            message: "b1".to_string(),
            praxis_error_info: None,
            additional_details: None,
        },
        &thread_state,
    )
    .await;
    handle_turn_complete(conversation_b, b_turn1.clone(), &outgoing, &thread_state).await;

    // Turn 2 on conversation A
    let a_turn2 = "a_turn2".to_string();
    handle_turn_complete(conversation_a, a_turn2.clone(), &outgoing, &thread_state).await;

    // Verify: A turn 1
    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
            assert_eq!(n.turn.id, a_turn1);
            assert_eq!(n.turn.status, TurnStatus::Failed);
            assert_eq!(
                n.turn.error,
                Some(TurnError {
                    message: "a1".to_string(),
                    praxis_error_info: Some(ApiPraxisErrorInfo::BadRequest),
                    additional_details: None,
                })
            );
        }
        other => bail!("unexpected message: {other:?}"),
    }

    // Verify: B turn 1
    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
            assert_eq!(n.turn.id, b_turn1);
            assert_eq!(n.turn.status, TurnStatus::Failed);
            assert_eq!(
                n.turn.error,
                Some(TurnError {
                    message: "b1".to_string(),
                    praxis_error_info: None,
                    additional_details: None,
                })
            );
        }
        other => bail!("unexpected message: {other:?}"),
    }

    // Verify: A turn 2
    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
            assert_eq!(n.turn.id, a_turn2);
            assert_eq!(n.turn.status, TurnStatus::Completed);
            assert_eq!(n.turn.error, None);
        }
        other => bail!("unexpected message: {other:?}"),
    }

    assert!(rx.try_recv().is_err(), "no extra messages expected");
    Ok(())
}

#[tokio::test]
async fn test_handle_turn_diff_emits_API_notification() -> Result<()> {
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], ThreadId::new());
    let unified_diff = "--- a\n+++ b\n".to_string();
    let conversation_id = ThreadId::new();
    let conversation_id_text = conversation_id.to_string();
    let workspace_change_store = WorkspaceChangeStore::default();

    handle_turn_diff(
        conversation_id,
        "turn-1",
        TurnDiffEvent {
            unified_diff: unified_diff.clone(),
        },
        &outgoing,
        PathBuf::from("."),
        &workspace_change_store,
    )
    .await;

    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::TurnDiffUpdated(
            notification,
        )) => {
            assert_eq!(notification.thread_id, conversation_id_text);
            assert_eq!(notification.turn_id, "turn-1");
            assert_eq!(notification.diff, unified_diff);
        }
        other => bail!("unexpected message: {other:?}"),
    }
    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::WorkspaceChangeUpdated(
            notification,
        )) => {
            assert_eq!(notification.thread_id, conversation_id_text);
            assert_eq!(
                notification.snapshot.thread_id.as_deref(),
                Some(conversation_id_text.as_str())
            );
            assert_eq!(notification.snapshot.turn_id.as_deref(), Some("turn-1"));
        }
        other => bail!("unexpected message: {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "no extra messages expected");
    Ok(())
}

#[tokio::test]
async fn test_hook_prompt_raw_response_emits_item_completed() -> Result<()> {
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let outgoing = Arc::new(OutgoingMessageSender::new(tx));
    let conversation_id = ThreadId::new();
    let outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing, vec![ConnectionId(1)], conversation_id);
    let item = build_hook_prompt_message(&[
        HookPromptFragment::from_single_hook("Retry with tests.", "hook-run-1"),
        HookPromptFragment::from_single_hook("Then summarize cleanly.", "hook-run-2"),
    ])
    .expect("hook prompt message");

    maybe_emit_hook_prompt_item_completed(conversation_id, "turn-1", &item, &outgoing).await;

    let msg = recv_broadcast_message(&mut rx).await?;
    match msg {
        OutgoingMessage::AppGatewayNotification(ServerNotification::ItemCompleted(
            notification,
        )) => {
            assert_eq!(notification.thread_id, conversation_id.to_string());
            assert_eq!(notification.turn_id, "turn-1");
            assert_eq!(
                notification.item,
                ThreadItem::HookPrompt {
                    id: notification.item.id().to_string(),
                    fragments: vec![
                        praxis_app_gateway_protocol::HookPromptFragment {
                            text: "Retry with tests.".into(),
                            hook_run_id: "hook-run-1".into(),
                        },
                        praxis_app_gateway_protocol::HookPromptFragment {
                            text: "Then summarize cleanly.".into(),
                            hook_run_id: "hook-run-2".into(),
                        },
                    ],
                }
            );
        }
        other => bail!("unexpected message: {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "no extra messages expected");
    Ok(())
}

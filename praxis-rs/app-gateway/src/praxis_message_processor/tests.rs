use super::*;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use crate::server_request_lifecycle::send_server_request;
use anyhow::Result;
use praxis_app_gateway_protocol::ServerRequestPayload;
use praxis_app_gateway_protocol::ToolRequestUserInputParams;
use praxis_core::config_loader::CloudRequirementsLoadError;
use praxis_core::config_loader::CloudRequirementsLoadErrorCode;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::protocol::USER_MESSAGE_BEGIN;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn validate_dynamic_tools_rejects_unsupported_input_schema() {
    let tools = vec![ApiDynamicToolSpec {
        name: "my_tool".to_string(),
        description: "test".to_string(),
        input_schema: json!({"type": "null"}),
        defer_loading: false,
    }];
    let err = validate_dynamic_tools(&tools).expect_err("invalid schema");
    assert!(err.contains("my_tool"), "unexpected error: {err}");
}

#[test]
fn validate_dynamic_tools_accepts_sanitizable_input_schema() {
    let tools = vec![ApiDynamicToolSpec {
        name: "my_tool".to_string(),
        description: "test".to_string(),
        // Missing `type` is common; core sanitizes these to a supported schema.
        input_schema: json!({"properties": {}}),
        defer_loading: false,
    }];
    validate_dynamic_tools(&tools).expect("valid schema");
}

#[test]
fn config_load_error_marks_cloud_requirements_failures_for_relogin() {
    let err = std::io::Error::other(CloudRequirementsLoadError::new(
        CloudRequirementsLoadErrorCode::Auth,
        Some(401),
        "Your authentication session could not be refreshed automatically. Please log out and sign in again.",
    ));

    let error = config_load_error(&err);

    assert_eq!(
        error.data,
        Some(json!({
            "reason": "cloudRequirements",
            "errorCode": "Auth",
            "action": "relogin",
            "statusCode": 401,
            "detail": "Your authentication session could not be refreshed automatically. Please log out and sign in again.",
        }))
    );
    assert!(
        error.message.contains("failed to load configuration"),
        "unexpected error message: {}",
        error.message
    );
}

#[test]
fn config_load_error_leaves_non_cloud_requirements_failures_unmarked() {
    let err = std::io::Error::other("required MCP servers failed to initialize");

    let error = config_load_error(&err);

    assert_eq!(error.data, None);
    assert!(
        error.message.contains("failed to load configuration"),
        "unexpected error message: {}",
        error.message
    );
}

#[test]
fn config_load_error_marks_non_auth_cloud_requirements_failures_without_relogin() {
    let err = std::io::Error::other(CloudRequirementsLoadError::new(
        CloudRequirementsLoadErrorCode::RequestFailed,
        /*status_code*/ None,
        "failed to load your workspace-managed config",
    ));

    let error = config_load_error(&err);

    assert_eq!(
        error.data,
        Some(json!({
            "reason": "cloudRequirements",
            "errorCode": "RequestFailed",
            "detail": "failed to load your workspace-managed config",
        }))
    );
}

#[test]
fn collect_resume_override_mismatches_includes_service_tier() {
    let request = ThreadResumeParams {
        thread_id: "thread-1".to_string(),
        history: None,
        path: None,
        model: None,
        model_provider: None,
        service_tier: Some(Some(praxis_protocol::config_types::ServiceTier::Fast)),
        cwd: None,
        approval_policy: None,
        approvals_reviewer: None,
        sandbox: None,
        config: None,
        base_instructions: None,
        developer_instructions: None,
        personality: None,
        persist_extended_history: false,
    };
    let config_snapshot = ThreadConfigSnapshot {
        model: "gpt-5".to_string(),
        model_provider_id: "openai".to_string(),
        service_tier: Some(praxis_protocol::config_types::ServiceTier::Flex),
        approval_policy: praxis_protocol::protocol::AskForApproval::OnRequest,
        approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer::User,
        sandbox_policy: praxis_protocol::protocol::SandboxPolicy::DangerFullAccess,
        cwd: PathBuf::from("/tmp"),
        ephemeral: false,
        reasoning_effort: None,
        personality: None,
        session_source: SessionSource::Cli,
    };

    assert_eq!(
        collect_resume_override_mismatches(&request, &config_snapshot),
        vec!["service_tier requested=Some(Fast) active=Some(Flex)".to_string()]
    );
}

fn test_thread_metadata(
    model: Option<&str>,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<ThreadMetadata> {
    let thread_id = ThreadId::from_string("3f941c35-29b3-493b-b0a4-e25800d9aeb0")?;
    let mut builder = ThreadMetadataBuilder::new(
        thread_id,
        PathBuf::from("/tmp/rollout.jsonl"),
        Utc::now(),
        praxis_protocol::protocol::SessionSource::default(),
    );
    builder.model_provider = Some("mock_provider".to_string());
    let mut metadata = builder.build("mock_provider");
    metadata.model = model.map(ToString::to_string);
    metadata.reasoning_effort = reasoning_effort;
    Ok(metadata)
}

#[test]
fn merge_persisted_resume_metadata_prefers_persisted_model_and_reasoning_effort() -> Result<()> {
    let mut request_overrides = None;
    let mut typesafe_overrides = ConfigOverrides::default();
    let persisted_metadata =
        test_thread_metadata(Some("gpt-5.1-codex-max"), Some(ReasoningEffort::High))?;

    merge_persisted_resume_metadata(
        &mut request_overrides,
        &mut typesafe_overrides,
        &persisted_metadata,
    );

    assert_eq!(
        typesafe_overrides.model,
        Some("gpt-5.1-codex-max".to_string())
    );
    assert_eq!(
        request_overrides,
        Some(HashMap::from([(
            "model_reasoning_effort".to_string(),
            serde_json::Value::String("high".to_string()),
        )]))
    );
    Ok(())
}

#[test]
fn merge_persisted_resume_metadata_preserves_explicit_overrides() -> Result<()> {
    let mut request_overrides = Some(HashMap::from([(
        "model_reasoning_effort".to_string(),
        serde_json::Value::String("low".to_string()),
    )]));
    let mut typesafe_overrides = ConfigOverrides {
        model: Some("gpt-5.2-codex".to_string()),
        ..Default::default()
    };
    let persisted_metadata =
        test_thread_metadata(Some("gpt-5.1-codex-max"), Some(ReasoningEffort::High))?;

    merge_persisted_resume_metadata(
        &mut request_overrides,
        &mut typesafe_overrides,
        &persisted_metadata,
    );

    assert_eq!(typesafe_overrides.model, Some("gpt-5.2-codex".to_string()));
    assert_eq!(
        request_overrides,
        Some(HashMap::from([(
            "model_reasoning_effort".to_string(),
            serde_json::Value::String("low".to_string()),
        )]))
    );
    Ok(())
}

#[test]
fn merge_persisted_resume_metadata_skips_persisted_values_when_model_overridden() -> Result<()> {
    let mut request_overrides = Some(HashMap::from([(
        "model".to_string(),
        serde_json::Value::String("gpt-5.2-codex".to_string()),
    )]));
    let mut typesafe_overrides = ConfigOverrides::default();
    let persisted_metadata =
        test_thread_metadata(Some("gpt-5.1-codex-max"), Some(ReasoningEffort::High))?;

    merge_persisted_resume_metadata(
        &mut request_overrides,
        &mut typesafe_overrides,
        &persisted_metadata,
    );

    assert_eq!(typesafe_overrides.model, None);
    assert_eq!(
        request_overrides,
        Some(HashMap::from([(
            "model".to_string(),
            serde_json::Value::String("gpt-5.2-codex".to_string()),
        )]))
    );
    Ok(())
}

#[test]
fn merge_persisted_resume_metadata_skips_persisted_values_when_provider_overridden() -> Result<()> {
    let mut request_overrides = None;
    let mut typesafe_overrides = ConfigOverrides {
        model_provider: Some("oss".to_string()),
        ..Default::default()
    };
    let persisted_metadata =
        test_thread_metadata(Some("gpt-5.1-codex-max"), Some(ReasoningEffort::High))?;

    merge_persisted_resume_metadata(
        &mut request_overrides,
        &mut typesafe_overrides,
        &persisted_metadata,
    );

    assert_eq!(typesafe_overrides.model, None);
    assert_eq!(typesafe_overrides.model_provider, Some("oss".to_string()));
    assert_eq!(request_overrides, None);
    Ok(())
}

#[test]
fn merge_persisted_resume_metadata_skips_persisted_values_when_reasoning_effort_overridden()
-> Result<()> {
    let mut request_overrides = Some(HashMap::from([(
        "model_reasoning_effort".to_string(),
        serde_json::Value::String("low".to_string()),
    )]));
    let mut typesafe_overrides = ConfigOverrides::default();
    let persisted_metadata =
        test_thread_metadata(Some("gpt-5.1-codex-max"), Some(ReasoningEffort::High))?;

    merge_persisted_resume_metadata(
        &mut request_overrides,
        &mut typesafe_overrides,
        &persisted_metadata,
    );

    assert_eq!(typesafe_overrides.model, None);
    assert_eq!(
        request_overrides,
        Some(HashMap::from([(
            "model_reasoning_effort".to_string(),
            serde_json::Value::String("low".to_string()),
        )]))
    );
    Ok(())
}

#[test]
fn merge_persisted_resume_metadata_skips_missing_values() -> Result<()> {
    let mut request_overrides = None;
    let mut typesafe_overrides = ConfigOverrides::default();
    let persisted_metadata =
        test_thread_metadata(/*model*/ None, /*reasoning_effort*/ None)?;

    merge_persisted_resume_metadata(
        &mut request_overrides,
        &mut typesafe_overrides,
        &persisted_metadata,
    );

    assert_eq!(typesafe_overrides.model, None);
    assert_eq!(request_overrides, None);
    Ok(())
}

#[test]
fn extract_conversation_summary_prefers_plain_user_messages() -> Result<()> {
    let conversation_id = ThreadId::from_string("3f941c35-29b3-493b-b0a4-e25800d9aeb0")?;
    let timestamp = Some("2025-09-05T16:53:11.850Z".to_string());
    let path = PathBuf::from("rollout.jsonl");

    let head = vec![
        json!({
            "id": conversation_id.to_string(),
            "timestamp": timestamp,
            "cwd": "/",
            "originator": "codex",
            "cli_version": "0.0.0",
            "model_provider": "test-provider"
        }),
        json!({
            "type": "message",
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": "# AGENTS.md instructions for project\n\n<INSTRUCTIONS>\n<AGENTS.md contents>\n</INSTRUCTIONS>".to_string(),
            }],
        }),
        json!({
            "type": "message",
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": format!("<prior context> {USER_MESSAGE_BEGIN}Count to 5"),
            }],
        }),
    ];

    let session_meta = serde_json::from_value::<SessionMeta>(head[0].clone())?;

    let summary = extract_rollout_summary(
        path.clone(),
        &head,
        &session_meta,
        /*git*/ None,
        "test-provider",
        timestamp.clone(),
    )
    .expect("summary");

    let expected = ThreadStoreSummary {
        conversation_id,
        timestamp: timestamp.clone(),
        updated_at: timestamp,
        path,
        preview: "Count to 5".to_string(),
        summary: None,
        model_provider: "test-provider".to_string(),
        model: None,
        cwd: PathBuf::from("/"),
        cli_version: "0.0.0".to_string(),
        source: SessionSource::VSCode,
        total_cost_usd: None,
        last_cost_usd: None,
        token_usage: None,
        selfwork_plan_path: None,
        git_info: None,
        thread_name: None,
    };

    assert_eq!(summary, expected);
    Ok(())
}

#[tokio::test]
async fn read_summary_from_rollout_returns_empty_preview_when_no_user_message() -> Result<()> {
    use praxis_protocol::protocol::RolloutItem;
    use praxis_protocol::protocol::RolloutLine;
    use praxis_protocol::protocol::SessionMetaLine;
    use std::fs;
    use std::fs::FileTimes;

    let temp_dir = TempDir::new()?;
    let path = temp_dir.path().join("rollout.jsonl");

    let conversation_id = ThreadId::from_string("bfd12a78-5900-467b-9bc5-d3d35df08191")?;
    let timestamp = "2025-09-05T16:53:11.850Z".to_string();

    let session_meta = SessionMeta {
        id: conversation_id,
        timestamp: timestamp.clone(),
        model_provider: None,
        ..SessionMeta::default()
    };

    let line = RolloutLine {
        timestamp: timestamp.clone(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: session_meta.clone(),
            git: None,
        }),
    };

    fs::write(&path, format!("{}\n", serde_json::to_string(&line)?))?;
    let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp)?.with_timezone(&Utc);
    let times = FileTimes::new().set_modified(parsed.into());
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)?
        .set_times(times)?;

    let summary = read_summary_from_rollout(path.as_path(), "fallback").await?;

    let expected = ThreadStoreSummary {
        conversation_id,
        timestamp: Some(timestamp.clone()),
        updated_at: Some("2025-09-05T16:53:11Z".to_string()),
        path: path.clone(),
        preview: String::new(),
        summary: None,
        model_provider: "fallback".to_string(),
        model: None,
        cwd: PathBuf::new(),
        cli_version: String::new(),
        source: SessionSource::VSCode,
        total_cost_usd: None,
        last_cost_usd: None,
        token_usage: None,
        selfwork_plan_path: None,
        git_info: None,
        thread_name: None,
    };

    assert_eq!(summary, expected);
    Ok(())
}

#[tokio::test]
async fn read_summary_from_rollout_uses_event_user_message_preview() -> Result<()> {
    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::RolloutItem;
    use praxis_protocol::protocol::RolloutLine;
    use praxis_protocol::protocol::SessionMetaLine;
    use praxis_protocol::protocol::UserMessageEvent;
    use std::fs;

    let temp_dir = TempDir::new()?;
    let path = temp_dir.path().join("rollout.jsonl");

    let conversation_id = ThreadId::from_string("bfd12a78-5900-467b-9bc5-d3d35df08191")?;
    let timestamp = "2025-09-05T16:53:11.850Z".to_string();

    let session_meta = SessionMeta {
        id: conversation_id,
        timestamp: timestamp.clone(),
        model_provider: Some("test-provider".to_string()),
        ..SessionMeta::default()
    };

    let meta_line = RolloutLine {
        timestamp: timestamp.clone(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: session_meta,
            git: None,
        }),
    };
    let user_line = RolloutLine {
        timestamp: timestamp.clone(),
        item: RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: format!("{USER_MESSAGE_BEGIN} actual user request"),
            images: Some(vec![]),
            local_images: Vec::new(),
            text_elements: Vec::new(),
        })),
    };
    fs::write(
        &path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&meta_line)?,
            serde_json::to_string(&user_line)?
        ),
    )?;

    let summary = read_summary_from_rollout(path.as_path(), "fallback").await?;

    assert_eq!(summary.preview, "actual user request");
    Ok(())
}

#[tokio::test]
async fn read_summary_from_rollout_preserves_agent_display_name() -> Result<()> {
    use praxis_protocol::protocol::RolloutItem;
    use praxis_protocol::protocol::RolloutLine;
    use praxis_protocol::protocol::SessionMetaLine;
    use std::fs;

    let temp_dir = TempDir::new()?;
    let path = temp_dir.path().join("rollout.jsonl");

    let conversation_id = ThreadId::from_string("bfd12a78-5900-467b-9bc5-d3d35df08191")?;
    let parent_thread_id = ThreadId::from_string("ad7f0408-99b8-4f6e-a46f-bd0eec433370")?;
    let timestamp = "2025-09-05T16:53:11.850Z".to_string();

    let session_meta = SessionMeta {
        id: conversation_id,
        timestamp: timestamp.clone(),
        source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth: 1,
            agent_path: None,
            agent_base_name: Some("墨子".to_string()),
            agent_title: Some("巡检仓库".to_string()),
            agent_display_name: None,
            agent_role: None,
        }),
        agent_base_name: Some("墨子".to_string()),
        agent_title: Some("巡检仓库".to_string()),
        agent_display_name: Some("atlas".to_string()),
        agent_role: Some("explorer".to_string()),
        model_provider: Some("test-provider".to_string()),
        ..SessionMeta::default()
    };

    let line = RolloutLine {
        timestamp,
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: session_meta,
            git: None,
        }),
    };
    fs::write(&path, format!("{}\n", serde_json::to_string(&line)?))?;

    let summary = read_summary_from_rollout(path.as_path(), "fallback").await?;
    let thread = summary_to_thread(summary);

    assert_eq!(thread.agent_display_name, Some("atlas".to_string()));
    assert_eq!(thread.agent_base_name, Some("墨子".to_string()));
    assert_eq!(thread.agent_title, Some("巡检仓库".to_string()));
    assert_eq!(thread.agent_role, Some("explorer".to_string()));
    Ok(())
}

#[tokio::test]
async fn aborting_pending_request_clears_pending_state() -> Result<()> {
    let thread_id = ThreadId::from_string("bfd12a78-5900-467b-9bc5-d3d35df08191")?;
    let connection_id = ConnectionId(7);

    let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::channel(8);
    let outgoing = Arc::new(OutgoingMessageSender::new(outgoing_tx));
    let thread_outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing.clone(), vec![connection_id], thread_id);

    let (request_id, client_request_rx) = thread_outgoing
        .send_request(ServerRequestPayload::ToolRequestUserInput(
            ToolRequestUserInputParams {
                thread_id: thread_id.to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "call-1".to_string(),
                questions: vec![],
            },
        ))
        .await;
    thread_outgoing.abort_pending_server_requests().await;

    let request_message = outgoing_rx.recv().await.expect("request should be sent");
    let OutgoingEnvelope::ToConnection {
        connection_id: request_connection_id,
        message:
            OutgoingMessage::Request(ServerRequest::ToolRequestUserInput {
                request_id: sent_request_id,
                ..
            }),
        ..
    } = request_message
    else {
        panic!("expected tool request to be sent to the subscribed connection");
    };
    assert_eq!(request_connection_id, connection_id);
    assert_eq!(sent_request_id, request_id);

    let response = client_request_rx
        .await
        .expect("callback should be resolved");
    let error = response.expect_err("request should be aborted during cleanup");
    assert_eq!(
        error.message,
        "client request resolved because the turn state was changed"
    );
    assert_eq!(error.data, Some(json!({ "reason": "turnTransition" })));
    assert!(
        outgoing
            .pending_requests_for_thread(thread_id)
            .await
            .is_empty()
    );
    assert!(outgoing_rx.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn server_request_send_uses_live_thread_subscribers() -> Result<()> {
    let manager = ThreadStateManager::new();
    let thread_id = ThreadId::new();
    let connection_id = ConnectionId(7);
    manager.connection_initialized(connection_id).await;
    let thread_state = manager
        .try_ensure_connection_subscribed(
            thread_id,
            connection_id,
            /*experimental_raw_events*/ false,
        )
        .await
        .expect("connection should be live");

    let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::channel(8);
    let outgoing = Arc::new(OutgoingMessageSender::new(outgoing_tx));
    let thread_outgoing = ThreadScopedOutgoingMessageSender::new(outgoing, Vec::new(), thread_id);

    let _pending_request = send_server_request(
        &manager,
        &thread_state,
        &thread_outgoing,
        ServerRequestPayload::ToolRequestUserInput(ToolRequestUserInputParams {
            thread_id: thread_id.to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "call-1".to_string(),
            questions: vec![],
        }),
    )
    .await;

    let request_message = outgoing_rx.recv().await.expect("request should be sent");
    let OutgoingEnvelope::ToConnection {
        connection_id: request_connection_id,
        message: OutgoingMessage::Request(ServerRequest::ToolRequestUserInput { .. }),
        ..
    } = request_message
    else {
        panic!("expected tool request to be sent to the live subscriber");
    };
    assert_eq!(request_connection_id, connection_id);
    Ok(())
}

#[tokio::test]
async fn pending_server_request_replays_after_late_subscription() -> Result<()> {
    let manager = ThreadStateManager::new();
    let thread_id = ThreadId::new();
    let connection_id = ConnectionId(9);
    let thread_state = manager.thread_state(thread_id).await;

    let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::channel(8);
    let outgoing = Arc::new(OutgoingMessageSender::new(outgoing_tx));
    let thread_outgoing =
        ThreadScopedOutgoingMessageSender::new(outgoing.clone(), Vec::new(), thread_id);

    let _pending_request = send_server_request(
        &manager,
        &thread_state,
        &thread_outgoing,
        ServerRequestPayload::ToolRequestUserInput(ToolRequestUserInputParams {
            thread_id: thread_id.to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "call-1".to_string(),
            questions: vec![],
        }),
    )
    .await;
    assert!(outgoing_rx.try_recv().is_err());

    let pending_requests = manager.pending_server_requests(thread_id).await;
    assert_eq!(pending_requests.len(), 1);
    let expected_request_id = pending_requests[0].id().clone();

    manager.connection_initialized(connection_id).await;
    manager
        .try_ensure_connection_subscribed(
            thread_id,
            connection_id,
            /*experimental_raw_events*/ false,
        )
        .await
        .expect("connection should be live");
    outgoing
        .replay_requests_to_connection_for_thread(connection_id, pending_requests)
        .await;

    let request_message = outgoing_rx.recv().await.expect("request should replay");
    let OutgoingEnvelope::ToConnection {
        connection_id: request_connection_id,
        message:
            OutgoingMessage::Request(ServerRequest::ToolRequestUserInput {
                request_id: replayed_request_id,
                ..
            }),
        ..
    } = request_message
    else {
        panic!("expected pending tool request to replay to late subscriber");
    };
    assert_eq!(request_connection_id, connection_id);
    assert_eq!(replayed_request_id, expected_request_id);
    Ok(())
}

#[test]
fn summary_from_state_db_metadata_preserves_agent_display_name() -> Result<()> {
    let conversation_id = ThreadId::from_string("bfd12a78-5900-467b-9bc5-d3d35df08191")?;
    let source = serde_json::to_string(&SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: ThreadId::from_string("ad7f0408-99b8-4f6e-a46f-bd0eec433370")?,
        depth: 1,
        agent_path: None,
        agent_base_name: None,
        agent_title: None,
        agent_display_name: None,
        agent_role: None,
    }))?;

    let summary = summary_from_state_db_metadata(
        conversation_id,
        PathBuf::from("/tmp/rollout.jsonl"),
        Some("hi".to_string()),
        None,
        "2025-09-05T16:53:11Z".to_string(),
        "2025-09-05T16:53:12Z".to_string(),
        "test-provider".to_string(),
        /*model*/ None,
        PathBuf::from("/"),
        "0.0.0".to_string(),
        source,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("atlas".to_string()),
        Some("explorer".to_string()),
        /*git_sha*/ None,
        /*git_branch*/ None,
        /*git_origin_url*/ None,
    );

    let thread = summary_to_thread(summary);

    assert_eq!(thread.agent_display_name, Some("atlas".to_string()));
    assert_eq!(thread.agent_role, Some("explorer".to_string()));
    Ok(())
}

#[tokio::test]
async fn removing_thread_state_clears_listener_and_active_turn_history() -> Result<()> {
    let manager = ThreadStateManager::new();
    let thread_id = ThreadId::from_string("ad7f0408-99b8-4f6e-a46f-bd0eec433370")?;
    let connection = ConnectionId(1);
    let (cancel_tx, cancel_rx) = oneshot::channel();

    manager.connection_initialized(connection).await;
    manager
        .try_ensure_connection_subscribed(
            thread_id, connection, /*experimental_raw_events*/ false,
        )
        .await
        .expect("connection should be live");
    {
        let state = manager.thread_state(thread_id).await;
        let mut state = state.lock().await;
        state.cancel_tx = Some(cancel_tx);
        state.track_current_turn_event(&EventMsg::TurnStarted(
            praxis_protocol::protocol::TurnStartedEvent {
                turn_id: "turn-1".to_string(),
                model_context_window: None,
                collaboration_mode_kind: Default::default(),
            },
        ));
    }

    manager.remove_thread_state(thread_id).await;
    assert_eq!(cancel_rx.await, Ok(()));

    let state = manager.thread_state(thread_id).await;
    let state = state.lock().await;
    assert!(
        manager
            .subscribed_connection_ids(thread_id)
            .await
            .is_empty()
    );
    assert!(state.cancel_tx.is_none());
    assert!(state.active_turn_snapshot().is_none());
    Ok(())
}

#[tokio::test]
async fn removing_auto_attached_connection_preserves_listener_for_other_connections() -> Result<()>
{
    let manager = ThreadStateManager::new();
    let thread_id = ThreadId::from_string("ad7f0408-99b8-4f6e-a46f-bd0eec433370")?;
    let connection_a = ConnectionId(1);
    let connection_b = ConnectionId(2);
    let (cancel_tx, mut cancel_rx) = oneshot::channel();

    manager.connection_initialized(connection_a).await;
    manager.connection_initialized(connection_b).await;
    manager
        .try_ensure_connection_subscribed(
            thread_id,
            connection_a,
            /*experimental_raw_events*/ false,
        )
        .await
        .expect("connection_a should be live");
    manager
        .try_ensure_connection_subscribed(
            thread_id,
            connection_b,
            /*experimental_raw_events*/ false,
        )
        .await
        .expect("connection_b should be live");
    {
        let state = manager.thread_state(thread_id).await;
        state.lock().await.cancel_tx = Some(cancel_tx);
    }

    manager.remove_connection(connection_a).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut cancel_rx)
            .await
            .is_err()
    );

    assert_eq!(
        manager.subscribed_connection_ids(thread_id).await,
        vec![connection_b]
    );
    Ok(())
}

#[tokio::test]
async fn closed_connection_cannot_be_reintroduced_by_auto_subscribe() -> Result<()> {
    let manager = ThreadStateManager::new();
    let thread_id = ThreadId::from_string("ad7f0408-99b8-4f6e-a46f-bd0eec433370")?;
    let connection = ConnectionId(1);

    manager.connection_initialized(connection).await;
    manager.remove_connection(connection).await;

    assert!(
        manager
            .try_ensure_connection_subscribed(
                thread_id, connection, /*experimental_raw_events*/ false
            )
            .await
            .is_none()
    );
    assert!(!manager.has_subscribers(thread_id).await);
    Ok(())
}

use anyhow::Context;
use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::create_shell_command_sse_response;
use app_test_support::to_response;
use core_test_support::responses;
use praxis_app_gateway_protocol::ItemStartedNotification;
use praxis_app_gateway_protocol::JSONRPCNotification;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadLoadedListParams;
use praxis_app_gateway_protocol::ThreadLoadedListResponse;
use praxis_app_gateway_protocol::ThreadReadParams;
use praxis_app_gateway_protocol::ThreadReadResponse;
use praxis_app_gateway_protocol::ThreadResumeParams;
use praxis_app_gateway_protocol::ThreadResumeResponse;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::ThreadStatusChangedNotification;
use praxis_app_gateway_protocol::ThreadUnsubscribeParams;
use praxis_app_gateway_protocol::ThreadUnsubscribeResponse;
use praxis_app_gateway_protocol::ThreadUnsubscribeStatus;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::UserInput as ApiUserInput;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

async fn wait_for_responses_request_count_to_stabilize(
    server: &wiremock::MockServer,
    expected_count: usize,
    settle_duration: std::time::Duration,
) -> Result<()> {
    timeout(DEFAULT_READ_TIMEOUT, async {
        let mut stable_since: Option<tokio::time::Instant> = None;
        loop {
            let requests = server
                .received_requests()
                .await
                .context("failed to fetch received requests")?;
            let responses_request_count = requests
                .iter()
                .filter(|request| {
                    request.method == "POST" && request.url.path().ends_with("/responses")
                })
                .count();

            if responses_request_count > expected_count {
                anyhow::bail!(
                    "expected exactly {expected_count} /responses requests, got {responses_request_count}"
                );
            }

            if responses_request_count == expected_count {
                match stable_since {
                    Some(stable_since) if stable_since.elapsed() >= settle_duration => {
                        return Ok::<(), anyhow::Error>(());
                    }
                    None => stable_since = Some(tokio::time::Instant::now()),
                    Some(_) => {}
                }
            } else {
                stable_since = None;
            }

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;

    Ok(())
}

#[tokio::test]
async fn thread_unsubscribe_unloads_thread_and_emits_thread_closed_notification() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_id = start_thread(&mut mcp).await?;

    let unsubscribe_id = mcp
        .send_thread_unsubscribe_request(ThreadUnsubscribeParams {
            thread_id: thread_id.clone(),
        })
        .await?;
    let unsubscribe_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(unsubscribe_id)),
    )
    .await??;
    let unsubscribe = to_response::<ThreadUnsubscribeResponse>(unsubscribe_resp)?;
    assert_eq!(unsubscribe.status, ThreadUnsubscribeStatus::Unsubscribed);

    let closed_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/closed"),
    )
    .await??;
    let parsed: ServerNotification = closed_notif.try_into()?;
    let ServerNotification::ThreadClosed(payload) = parsed else {
        anyhow::bail!("expected thread/closed notification");
    };
    assert_eq!(payload.thread_id, thread_id);

    let status_changed = wait_for_thread_status_not_loaded(&mut mcp, &payload.thread_id).await?;
    assert_eq!(status_changed.thread_id, payload.thread_id);
    assert_eq!(status_changed.status, ThreadStatus::NotLoaded);

    let list_id = mcp
        .send_thread_loaded_list_request(ThreadLoadedListParams::default())
        .await?;
    let list_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(list_id)),
    )
    .await??;
    let ThreadLoadedListResponse { data, next_cursor } =
        to_response::<ThreadLoadedListResponse>(list_resp)?;
    assert_eq!(data, Vec::<String>::new());
    assert_eq!(next_cursor, None);

    Ok(())
}

#[tokio::test]
async fn thread_unsubscribe_during_turn_interrupts_turn_and_emits_thread_closed() -> Result<()> {
    #[cfg(target_os = "windows")]
    let shell_command = vec![
        "powershell".to_string(),
        "-Command".to_string(),
        "Start-Sleep -Seconds 10".to_string(),
    ];
    #[cfg(not(target_os = "windows"))]
    let shell_command = vec!["sleep".to_string(), "10".to_string()];

    let tmp = TempDir::new()?;
    let praxis_home = tmp.path().join("praxis_home");
    std::fs::create_dir(&praxis_home)?;
    let working_directory = tmp.path().join("workdir");
    std::fs::create_dir(&working_directory)?;

    let server = create_mock_responses_server_sequence_unchecked(vec![
        create_shell_command_sse_response(
            shell_command.clone(),
            Some(&working_directory),
            Some(10_000),
            "call_sleep",
        )?,
        create_final_assistant_message_sse_response("Done")?,
    ])
    .await;
    create_config_toml(&praxis_home, &server.uri())?;

    let mut mcp = McpProcess::new(&praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_id = start_thread(&mut mcp).await?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![ApiUserInput::Text {
                text: "run sleep".to_string(),
                text_elements: Vec::new(),
            }],
            cwd: Some(working_directory),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;

    timeout(
        DEFAULT_READ_TIMEOUT,
        wait_for_command_execution_item_started(&mut mcp),
    )
    .await??;

    let unsubscribe_id = mcp
        .send_thread_unsubscribe_request(ThreadUnsubscribeParams {
            thread_id: thread_id.clone(),
        })
        .await?;
    let unsubscribe_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(unsubscribe_id)),
    )
    .await??;
    let unsubscribe = to_response::<ThreadUnsubscribeResponse>(unsubscribe_resp)?;
    assert_eq!(unsubscribe.status, ThreadUnsubscribeStatus::Unsubscribed);

    let closed_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/closed"),
    )
    .await??;
    let parsed: ServerNotification = closed_notif.try_into()?;
    let ServerNotification::ThreadClosed(payload) = parsed else {
        anyhow::bail!("expected thread/closed notification");
    };
    assert_eq!(payload.thread_id, thread_id);

    wait_for_responses_request_count_to_stabilize(
        &server,
        /*expected_count*/ 1,
        std::time::Duration::from_millis(200),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn thread_unsubscribe_clears_cached_status_before_resume() -> Result<()> {
    let server = responses::start_mock_server().await;
    let _response_mock = responses::mount_sse_once(
        &server,
        responses::sse_failed("resp-1", "server_error", "simulated failure"),
    )
    .await;
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_id = start_thread(&mut mcp).await?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![ApiUserInput::Text {
                text: "fail this turn".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("error"),
    )
    .await??;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread_id.clone(),
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;
    assert_eq!(thread.status, ThreadStatus::SystemError);

    let unsubscribe_id = mcp
        .send_thread_unsubscribe_request(ThreadUnsubscribeParams {
            thread_id: thread_id.clone(),
        })
        .await?;
    let unsubscribe_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(unsubscribe_id)),
    )
    .await??;
    let unsubscribe = to_response::<ThreadUnsubscribeResponse>(unsubscribe_resp)?;
    assert_eq!(unsubscribe.status, ThreadUnsubscribeStatus::Unsubscribed);
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/closed"),
    )
    .await??;

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id,
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let resume: ThreadResumeResponse = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(resume.thread.status, ThreadStatus::Idle);

    Ok(())
}

#[tokio::test]
async fn thread_unsubscribe_reports_not_loaded_after_thread_is_unloaded() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_id = start_thread(&mut mcp).await?;

    let first_unsubscribe_id = mcp
        .send_thread_unsubscribe_request(ThreadUnsubscribeParams {
            thread_id: thread_id.clone(),
        })
        .await?;
    let first_unsubscribe_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(first_unsubscribe_id)),
    )
    .await??;
    let first_unsubscribe = to_response::<ThreadUnsubscribeResponse>(first_unsubscribe_resp)?;
    assert_eq!(
        first_unsubscribe.status,
        ThreadUnsubscribeStatus::Unsubscribed
    );

    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/closed"),
    )
    .await??;

    let second_unsubscribe_id = mcp
        .send_thread_unsubscribe_request(ThreadUnsubscribeParams { thread_id })
        .await?;
    let second_unsubscribe_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(second_unsubscribe_id)),
    )
    .await??;
    let second_unsubscribe = to_response::<ThreadUnsubscribeResponse>(second_unsubscribe_resp)?;
    assert_eq!(
        second_unsubscribe.status,
        ThreadUnsubscribeStatus::NotLoaded
    );

    Ok(())
}

async fn wait_for_command_execution_item_started(mcp: &mut McpProcess) -> Result<()> {
    loop {
        let started_notif = mcp
            .read_stream_until_notification_message("item/started")
            .await?;
        let started_params = started_notif.params.context("item/started params")?;
        let started: ItemStartedNotification = serde_json::from_value(started_params)?;
        if let ThreadItem::CommandExecution { .. } = started.item {
            return Ok(());
        }
    }
}

async fn wait_for_thread_status_not_loaded(
    mcp: &mut McpProcess,
    thread_id: &str,
) -> Result<ThreadStatusChangedNotification> {
    loop {
        let status_changed_notif: JSONRPCNotification = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_notification_message("thread/status/changed"),
        )
        .await??;
        let status_changed_params = status_changed_notif
            .params
            .context("thread/status/changed params must be present")?;
        let status_changed: ThreadStatusChangedNotification =
            serde_json::from_value(status_changed_params)?;
        if status_changed.thread_id == thread_id && status_changed.status == ThreadStatus::NotLoaded
        {
            return Ok(status_changed);
        }
    }
}

fn create_config_toml(praxis_home: &std::path::Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = praxis_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "danger-full-access"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}

async fn start_thread(mcp: &mut McpProcess) -> Result<String> {
    let req_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(req_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(resp)?;
    Ok(thread.id)
}

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial(mcp_test_value)]
async fn stdio_server_round_trip() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;

    let call_id = "call-123";
    let server_name = "rmcp";
    let tool_name = format!("mcp__{server_name}__echo");

    mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_function_call(call_id, &tool_name, "{\"message\":\"ping\"}"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_assistant_message("msg-1", "rmcp echo tool completed successfully."),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    let expected_env_value = "propagated-env";
    let rmcp_test_server_bin = stdio_server_bin()?;

    let fixture = test_praxis()
        .with_config(move |config| {
            let mut servers = config.mcp_servers.get().clone();
            servers.insert(
                server_name.to_string(),
                McpServerConfig {
                    transport: McpServerTransportConfig::Stdio {
                        command: rmcp_test_server_bin,
                        args: Vec::new(),
                        env: Some(HashMap::from([(
                            "MCP_TEST_VALUE".to_string(),
                            expected_env_value.to_string(),
                        )])),
                        env_vars: Vec::new(),
                        cwd: None,
                    },
                    enabled: true,
                    required: false,
                    disabled_reason: None,
                    startup_timeout_sec: Some(Duration::from_secs(10)),
                    tool_timeout_sec: None,
                    enabled_tools: None,
                    disabled_tools: None,
                    scopes: None,
                    oauth_resource: None,
                    tools: HashMap::new(),
                },
            );
            config
                .mcp_servers
                .set(servers)
                .expect("test mcp servers should accept any configuration");
        })
        .build(&server)
        .await?;
    let session_model = fixture.session_configured.model.clone();

    fixture
        .thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "call the rmcp echo tool".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: fixture.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let begin_event = wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallBegin(_))
    })
    .await;

    let EventMsg::McpToolCallBegin(begin) = begin_event else {
        unreachable!("event guard guarantees McpToolCallBegin");
    };
    assert_eq!(begin.invocation.server, server_name);
    assert_eq!(begin.invocation.tool, "echo");

    let end_event = wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallEnd(_))
    })
    .await;
    let EventMsg::McpToolCallEnd(end) = end_event else {
        unreachable!("event guard guarantees McpToolCallEnd");
    };

    let result = end
        .result
        .as_ref()
        .expect("rmcp echo tool should return success");
    assert_eq!(result.is_error, Some(false));
    assert!(
        result.content.is_empty(),
        "content should default to an empty array"
    );

    let structured = result
        .structured_content
        .as_ref()
        .expect("structured content");
    let Value::Object(map) = structured else {
        panic!("structured content should be an object: {structured:?}");
    };
    let echo_value = map
        .get("echo")
        .and_then(Value::as_str)
        .expect("echo payload present");
    assert_eq!(echo_value, "ECHOING: ping");
    let env_value = map
        .get("env")
        .and_then(Value::as_str)
        .expect("env snapshot inserted");
    assert_eq!(env_value, expected_env_value);

    wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::TurnComplete(_))
    })
    .await;

    server.verify().await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial(mcp_test_value)]
async fn stdio_image_responses_round_trip() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;

    let call_id = "img-1";
    let server_name = "rmcp";
    let tool_name = format!("mcp__{server_name}__image");

    // First stream: model decides to call the image tool.
    mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_function_call(call_id, &tool_name, "{}"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    // Second stream: after tool execution, assistant emits a message and completes.
    let final_mock = mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_assistant_message("msg-1", "rmcp image tool completed successfully."),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    // Build the stdio rmcp server and pass the image as data URL so it can construct ImageContent.
    let rmcp_test_server_bin = stdio_server_bin()?;

    let fixture = test_praxis()
        .with_config(move |config| {
            let mut servers = config.mcp_servers.get().clone();
            servers.insert(
                server_name.to_string(),
                McpServerConfig {
                    transport: McpServerTransportConfig::Stdio {
                        command: rmcp_test_server_bin,
                        args: Vec::new(),
                        env: Some(HashMap::from([(
                            "MCP_TEST_IMAGE_DATA_URL".to_string(),
                            OPENAI_PNG.to_string(),
                        )])),
                        env_vars: Vec::new(),
                        cwd: None,
                    },
                    enabled: true,
                    required: false,
                    disabled_reason: None,
                    startup_timeout_sec: Some(Duration::from_secs(10)),
                    tool_timeout_sec: None,
                    enabled_tools: None,
                    disabled_tools: None,
                    scopes: None,
                    oauth_resource: None,
                    tools: HashMap::new(),
                },
            );
            config
                .mcp_servers
                .set(servers)
                .expect("test mcp servers should accept any configuration");
        })
        .build(&server)
        .await?;
    let session_model = fixture.session_configured.model.clone();

    let tools_ready_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        fixture.thread.submit(Op::ListMcpTools).await?;
        let list_event = wait_for_event_with_timeout(
            &fixture.thread,
            |ev| matches!(ev, EventMsg::McpListToolsResponse(_)),
            Duration::from_secs(10),
        )
        .await;
        let EventMsg::McpListToolsResponse(tool_list) = list_event else {
            unreachable!("event guard guarantees McpListToolsResponse");
        };
        if tool_list.tools.contains_key(&tool_name) {
            break;
        }

        let available_tools: Vec<&str> = tool_list.tools.keys().map(String::as_str).collect();
        if Instant::now() >= tools_ready_deadline {
            panic!(
                "timed out waiting for MCP tool {tool_name} to become available; discovered tools: {available_tools:?}"
            );
        }
        sleep(Duration::from_millis(200)).await;
    }

    fixture
        .thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "call the rmcp image tool".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: fixture.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    // Wait for tool begin/end and final completion.
    let begin_event = wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallBegin(_))
    })
    .await;
    let EventMsg::McpToolCallBegin(begin) = begin_event else {
        unreachable!("begin");
    };
    assert_eq!(
        begin,
        McpToolCallBeginEvent {
            call_id: call_id.to_string(),
            invocation: McpInvocation {
                server: server_name.to_string(),
                tool: "image".to_string(),
                arguments: Some(json!({})),
            },
        },
    );

    let end_event = wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallEnd(_))
    })
    .await;
    let EventMsg::McpToolCallEnd(end) = end_event else {
        unreachable!("end");
    };
    assert_eq!(end.call_id, call_id);
    assert_eq!(
        end.invocation,
        McpInvocation {
            server: server_name.to_string(),
            tool: "image".to_string(),
            arguments: Some(json!({})),
        }
    );
    let result = end.result.expect("rmcp image tool should return success");
    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.content.len(), 1);
    let base64_only = OPENAI_PNG
        .strip_prefix("data:image/png;base64,")
        .expect("data url prefix");
    let entry = result.content[0].as_object().expect("content object");
    assert_eq!(entry.get("type"), Some(&json!("image")));
    assert_eq!(entry.get("mimeType"), Some(&json!("image/png")));
    assert_eq!(entry.get("data"), Some(&json!(base64_only)));

    wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::TurnComplete(_))
    })
    .await;

    let output_item = final_mock.single_request().function_call_output(call_id);
    assert_eq!(
        output_item,
        json!({
            "type": "function_call_output",
            "call_id": call_id,
            "output": [{
                "type": "input_image",
                "image_url": OPENAI_PNG
            }]
        })
    );
    server.verify().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial(mcp_test_value)]
async fn stdio_image_responses_are_sanitized_for_text_only_model() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;

    let call_id = "img-text-only-1";
    let server_name = "rmcp";
    let tool_name = format!("mcp__{server_name}__image");
    let text_only_model_slug = "rmcp-text-only-model";

    let models_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: vec![ModelInfo {
                slug: text_only_model_slug.to_string(),
                display_name: "RMCP Text Only".to_string(),
                description: Some("Test model without image input support".to_string()),
                default_reasoning_level: None,
                supported_reasoning_levels: vec![ReasoningEffortPreset {
                    effort: praxis_protocol::openai_models::ReasoningEffort::Medium,
                    display_name: None,
                    description: "Medium".to_string(),
                }],
                shell_type: ConfigShellToolType::Default,
                visibility: ModelVisibility::List,
                supported_in_api: true,
                priority: 1,
                upgrade: None,
                base_instructions: "base instructions".to_string(),
                model_messages: None,
                supports_reasoning_summaries: false,
                default_reasoning_summary: ReasoningSummary::Auto,
                support_verbosity: false,
                default_verbosity: None,
                availability_nux: None,
                apply_patch_tool_type: None,
                web_search_tool_type: Default::default(),
                truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
                supports_parallel_tool_calls: false,
                supports_image_detail_original: false,
                context_window: Some(272_000),
                auto_compact_token_limit: None,
                effective_context_window_percent: 95,
                experimental_supported_tools: Vec::new(),
                input_modalities: vec![InputModality::Text],
                used_fallback_model_metadata: false,
                supports_search_tool: false,
                multi_agent_version: None,
            }],
        },
    )
    .await;

    // First stream: model decides to call the image tool.
    mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_function_call(call_id, &tool_name, "{}"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    // Second stream: after tool execution, assistant emits a message and completes.
    let final_mock = mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_assistant_message("msg-1", "rmcp image tool completed successfully."),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    let rmcp_test_server_bin = stdio_server_bin()?;

    let fixture = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            let mut servers = config.mcp_servers.get().clone();
            servers.insert(
                server_name.to_string(),
                McpServerConfig {
                    transport: McpServerTransportConfig::Stdio {
                        command: rmcp_test_server_bin,
                        args: Vec::new(),
                        env: Some(HashMap::from([(
                            "MCP_TEST_IMAGE_DATA_URL".to_string(),
                            OPENAI_PNG.to_string(),
                        )])),
                        env_vars: Vec::new(),
                        cwd: None,
                    },
                    enabled: true,
                    required: false,
                    disabled_reason: None,
                    startup_timeout_sec: Some(Duration::from_secs(10)),
                    tool_timeout_sec: None,
                    enabled_tools: None,
                    disabled_tools: None,
                    scopes: None,
                    oauth_resource: None,
                    tools: HashMap::new(),
                },
            );
            config
                .mcp_servers
                .set(servers)
                .expect("test mcp servers should accept any configuration");
        })
        .build(&server)
        .await?;

    fixture
        .thread_manager
        .get_models_manager()
        .list_models(RefreshStrategy::Online)
        .await;
    assert_eq!(models_mock.requests().len(), 1);

    fixture
        .thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "call the rmcp image tool".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: fixture.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: text_only_model_slug.to_string(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallBegin(_))
    })
    .await;
    wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallEnd(_))
    })
    .await;
    wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::TurnComplete(_))
    })
    .await;

    let output_item = final_mock.single_request().function_call_output(call_id);
    let output_text = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("function_call_output output should be a JSON string");
    let output_json: Value = serde_json::from_str(output_text)
        .expect("function_call_output output should be valid JSON");
    assert_eq!(
        output_json,
        json!([{
            "type": "text",
            "text": "<image content omitted because you do not support image input>"
        }])
    );
    server.verify().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial(mcp_test_value)]
async fn stdio_server_propagates_whitelisted_env_vars() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;

    let call_id = "call-1234";
    let server_name = "rmcp_whitelist";
    let tool_name = format!("mcp__{server_name}__echo");

    mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_function_call(call_id, &tool_name, "{\"message\":\"ping\"}"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_assistant_message("msg-1", "rmcp echo tool completed successfully."),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    let expected_env_value = "propagated-env-from-whitelist";
    let _guard = EnvVarGuard::set("MCP_TEST_VALUE", OsStr::new(expected_env_value));
    let rmcp_test_server_bin = stdio_server_bin()?;

    let fixture = test_praxis()
        .with_config(move |config| {
            let mut servers = config.mcp_servers.get().clone();
            servers.insert(
                server_name.to_string(),
                McpServerConfig {
                    transport: McpServerTransportConfig::Stdio {
                        command: rmcp_test_server_bin,
                        args: Vec::new(),
                        env: None,
                        env_vars: vec!["MCP_TEST_VALUE".to_string()],
                        cwd: None,
                    },
                    enabled: true,
                    required: false,
                    disabled_reason: None,
                    startup_timeout_sec: Some(Duration::from_secs(10)),
                    tool_timeout_sec: None,
                    enabled_tools: None,
                    disabled_tools: None,
                    scopes: None,
                    oauth_resource: None,
                    tools: HashMap::new(),
                },
            );
            config
                .mcp_servers
                .set(servers)
                .expect("test mcp servers should accept any configuration");
        })
        .build(&server)
        .await?;
    let session_model = fixture.session_configured.model.clone();

    fixture
        .thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "call the rmcp echo tool".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: fixture.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let begin_event = wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallBegin(_))
    })
    .await;

    let EventMsg::McpToolCallBegin(begin) = begin_event else {
        unreachable!("event guard guarantees McpToolCallBegin");
    };
    assert_eq!(begin.invocation.server, server_name);
    assert_eq!(begin.invocation.tool, "echo");

    let end_event = wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::McpToolCallEnd(_))
    })
    .await;
    let EventMsg::McpToolCallEnd(end) = end_event else {
        unreachable!("event guard guarantees McpToolCallEnd");
    };

    let result = end
        .result
        .as_ref()
        .expect("rmcp echo tool should return success");
    assert_eq!(result.is_error, Some(false));
    assert!(
        result.content.is_empty(),
        "content should default to an empty array"
    );

    let structured = result
        .structured_content
        .as_ref()
        .expect("structured content");
    let Value::Object(map) = structured else {
        panic!("structured content should be an object: {structured:?}");
    };
    let echo_value = map
        .get("echo")
        .and_then(Value::as_str)
        .expect("echo payload present");
    assert_eq!(echo_value, "ECHOING: ping");
    let env_value = map
        .get("env")
        .and_then(Value::as_str)
        .expect("env snapshot inserted");
    assert_eq!(env_value, expected_env_value);

    wait_for_event(&fixture.thread, |ev| {
        matches!(ev, EventMsg::TurnComplete(_))
    })
    .await;

    server.verify().await;

    Ok(())
}

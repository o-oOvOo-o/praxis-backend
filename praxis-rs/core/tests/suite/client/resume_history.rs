use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_includes_initial_messages_and_sends_prior_items() {
    skip_if_no_network!();

    // Create a fake rollout session file with prior user + system + assistant messages.
    let tmpdir = TempDir::new().unwrap();
    let session_path = tmpdir.path().join("resume-session.jsonl");
    let mut f = std::fs::File::create(&session_path).unwrap();
    let convo_id = Uuid::new_v4();
    writeln!(
        f,
        "{}",
        json!({
            "timestamp": "2024-01-01T00:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": convo_id,
                "timestamp": "2024-01-01T00:00:00Z",
                "instructions": "be nice",
                "cwd": ".",
                "originator": "test_originator",
                "cli_version": "test_version",
                "model_provider": "test-provider"
            }
        })
    )
    .unwrap();

    // Prior item: user message (should be delivered)
    let prior_user = praxis_protocol::models::ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![praxis_protocol::models::ContentItem::InputText {
            text: "resumed user message".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    let prior_user_json = serde_json::to_value(&prior_user).unwrap();
    writeln!(
        f,
        "{}",
        json!({
            "timestamp": "2024-01-01T00:00:01.000Z",
            "type": "response_item",
            "payload": prior_user_json
        })
    )
    .unwrap();

    // Prior item: system message (excluded from API history)
    let prior_system = praxis_protocol::models::ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![praxis_protocol::models::ContentItem::OutputText {
            text: "resumed system instruction".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    let prior_system_json = serde_json::to_value(&prior_system).unwrap();
    writeln!(
        f,
        "{}",
        json!({
            "timestamp": "2024-01-01T00:00:02.000Z",
            "type": "response_item",
            "payload": prior_system_json
        })
    )
    .unwrap();

    // Prior item: assistant message
    let prior_item = praxis_protocol::models::ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![praxis_protocol::models::ContentItem::OutputText {
            text: "resumed assistant message".to_string(),
        }],
        end_turn: None,
        phase: Some(MessagePhase::Commentary),
    };
    let prior_item_json = serde_json::to_value(&prior_item).unwrap();
    writeln!(
        f,
        "{}",
        json!({
            "timestamp": "2024-01-01T00:00:03.000Z",
            "type": "response_item",
            "payload": prior_item_json
        })
    )
    .unwrap();
    drop(f);

    // Mock server that will receive the resumed request
    let server = MockServer::start().await;
    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    // Configure Praxis to resume from our file
    let praxis_home = Arc::new(TempDir::new().unwrap());
    let mut builder = test_praxis()
        .with_home(praxis_home.clone())
        .with_config(|config| {
            // Ensure user instructions are NOT delivered on resume.
            config.user_instructions = Some("be nice".to_string());
        });
    let test = builder
        .resume(&server, praxis_home, session_path.clone())
        .await
        .expect("resume conversation");
    let praxis = test.thread.clone();
    let session_configured = test.session_configured;

    // 1) Assert initial_messages only includes existing EventMsg entries; response items are not converted
    let initial_msgs = session_configured
        .initial_messages
        .clone()
        .expect("expected initial messages option for resumed session");
    let initial_json = serde_json::to_value(&initial_msgs).unwrap();
    let expected_initial_json = json!([]);
    assert_eq!(initial_json, expected_initial_json);

    // 2) Submit new input; the request body must include the prior items, then initial context, then new user input.
    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = resp_mock.single_request();
    let request_body = request.body_json();
    let input = request_body["input"].as_array().expect("input array");
    let mut messages: Vec<(String, String)> = Vec::new();
    for item in input {
        let Some(role) = item.get("role").and_then(|role| role.as_str()) else {
            continue;
        };
        for text in message_input_texts(item) {
            messages.push((role.to_string(), text.to_string()));
        }
    }
    let pos_prior_user = messages
        .iter()
        .position(|(role, text)| role == "user" && text == "resumed user message")
        .expect("prior user message");
    let pos_prior_assistant = messages
        .iter()
        .position(|(role, text)| role == "assistant" && text == "resumed assistant message")
        .expect("prior assistant message");
    let prior_assistant = input
        .iter()
        .find(|item| {
            item.get("role").and_then(|role| role.as_str()) == Some("assistant")
                && item
                    .get("content")
                    .and_then(|content| content.as_array())
                    .and_then(|content| content.first())
                    .and_then(|entry| entry.get("text"))
                    .and_then(|text| text.as_str())
                    == Some("resumed assistant message")
        })
        .expect("resumed assistant message request item");
    assert_eq!(
        prior_assistant
            .get("phase")
            .and_then(|phase| phase.as_str()),
        Some("commentary")
    );
    let pos_permissions = messages
        .iter()
        .position(|(role, text)| role == "developer" && text.contains("<permissions instructions>"))
        .expect("permissions message");
    let pos_user_instructions = messages
        .iter()
        .position(|(role, text)| {
            role == "user"
                && text.contains("be nice")
                && (text.starts_with("# AGENTS.md instructions for "))
        })
        .expect("user instructions");
    let pos_environment = messages
        .iter()
        .position(|(role, text)| role == "user" && text.contains("<environment_context>"))
        .expect("environment context");
    let pos_new_user = messages
        .iter()
        .position(|(role, text)| role == "user" && text == "hello")
        .expect("new user message");

    assert!(pos_prior_user < pos_prior_assistant);
    assert!(pos_prior_assistant < pos_permissions);
    assert!(pos_permissions < pos_user_instructions);
    assert!(pos_user_instructions < pos_environment);
    assert!(pos_environment < pos_new_user);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_replays_legacy_js_repl_image_rollout_shapes() {
    skip_if_no_network!();

    // Early js_repl builds persisted image tool results as two separate rollout items:
    // a string-valued custom_tool_call_output plus a standalone user input_image message.
    // Current image tests cover today's shapes; this keeps resume compatibility for that
    // legacy rollout representation.
    let legacy_custom_tool_call = ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "legacy-js-call".to_string(),
        name: "js_repl".to_string(),
        input: "console.log('legacy image flow')".to_string(),
    };
    let legacy_image_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
    let rollout = vec![
        RolloutLine {
            timestamp: "2024-01-01T00:00:00.000Z".to_string(),
            item: RolloutItem::SessionMeta(SessionMetaLine {
                meta: SessionMeta {
                    id: ThreadId::default(),
                    timestamp: "2024-01-01T00:00:00Z".to_string(),
                    cwd: ".".into(),
                    originator: "test_originator".to_string(),
                    cli_version: "test_version".to_string(),
                    model_provider: Some("test-provider".to_string()),
                    ..Default::default()
                },
                git: None,
            }),
        },
        RolloutLine {
            timestamp: "2024-01-01T00:00:01.000Z".to_string(),
            item: RolloutItem::ResponseItem(legacy_custom_tool_call),
        },
        RolloutLine {
            timestamp: "2024-01-01T00:00:02.000Z".to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::CustomToolCallOutput {
                call_id: "legacy-js-call".to_string(),
                name: None,
                output: FunctionCallOutputPayload::from_text("legacy js_repl stdout".to_string()),
            }),
        },
        RolloutLine {
            timestamp: "2024-01-01T00:00:03.000Z".to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputImage {
                    image_url: legacy_image_url.to_string(),
                }],
                end_turn: None,
                phase: None,
            }),
        },
    ];

    let tmpdir = TempDir::new().unwrap();
    let session_path = tmpdir
        .path()
        .join("resume-legacy-js-repl-image-rollout.jsonl");
    let mut f = std::fs::File::create(&session_path).unwrap();
    for line in rollout {
        writeln!(f, "{}", serde_json::to_string(&line).unwrap()).unwrap();
    }

    let server = MockServer::start().await;
    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let praxis_home = Arc::new(TempDir::new().unwrap());
    let mut builder = test_praxis().with_model("gpt-5.1");
    let test = builder
        .resume(&server, praxis_home, session_path.clone())
        .await
        .expect("resume conversation");
    test.submit_turn("after resume").await.unwrap();

    let input = resp_mock.single_request().input();

    let legacy_output_index = input
        .iter()
        .position(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("custom_tool_call_output")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("legacy-js-call")
        })
        .expect("legacy custom tool output should be replayed");
    assert_eq!(
        input[legacy_output_index]
            .get("output")
            .and_then(|value| value.as_str()),
        Some("legacy js_repl stdout")
    );

    let legacy_image_index = input
        .iter()
        .position(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("message")
                && item.get("role").and_then(|value| value.as_str()) == Some("user")
                && item
                    .get("content")
                    .and_then(|value| value.as_array())
                    .is_some_and(|content| {
                        content.iter().any(|entry| {
                            entry.get("type").and_then(|value| value.as_str())
                                == Some("input_image")
                                && entry.get("image_url").and_then(|value| value.as_str())
                                    == Some(legacy_image_url)
                        })
                    })
        })
        .expect("legacy injected image message should be replayed");

    let new_user_index = input
        .iter()
        .position(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("message")
                && item.get("role").and_then(|value| value.as_str()) == Some("user")
                && item
                    .get("content")
                    .and_then(|value| value.as_array())
                    .is_some_and(|content| {
                        content.iter().any(|entry| {
                            entry.get("type").and_then(|value| value.as_str()) == Some("input_text")
                                && entry.get("text").and_then(|value| value.as_str())
                                    == Some("after resume")
                        })
                    })
        })
        .expect("new user message should be present");

    assert!(legacy_output_index < new_user_index);
    assert!(legacy_image_index < new_user_index);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_replays_image_tool_outputs_with_detail() {
    skip_if_no_network!();

    let image_url = "data:image/webp;base64,UklGRiIAAABXRUJQVlA4IBYAAAAwAQCdASoBAAEAAUAmJaACdLoB+AADsAD+8ut//NgVzXPv9//S4P0uD9Lg/9KQAAA=";
    let function_call_id = "view-image-call";
    let custom_call_id = "js-repl-call";
    let rollout = vec![
        RolloutLine {
            timestamp: "2024-01-01T00:00:00.000Z".to_string(),
            item: RolloutItem::SessionMeta(SessionMetaLine {
                meta: SessionMeta {
                    id: ThreadId::default(),
                    timestamp: "2024-01-01T00:00:00Z".to_string(),
                    cwd: ".".into(),
                    originator: "test_originator".to_string(),
                    cli_version: "test_version".to_string(),
                    model_provider: Some("test-provider".to_string()),
                    ..Default::default()
                },
                git: None,
            }),
        },
        RolloutLine {
            timestamp: "2024-01-01T00:00:01.000Z".to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "view_image".to_string(),
                namespace: None,
                arguments: "{\"path\":\"/tmp/example.webp\"}".to_string(),
                call_id: function_call_id.to_string(),
            }),
        },
        RolloutLine {
            timestamp: "2024-01-01T00:00:01.500Z".to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::FunctionCallOutput {
                call_id: function_call_id.to_string(),
                output: FunctionCallOutputPayload::from_content_items(vec![
                    FunctionCallOutputContentItem::InputImage {
                        image_url: image_url.to_string(),
                        detail: Some(ImageDetail::Original),
                    },
                ]),
            }),
        },
        RolloutLine {
            timestamp: "2024-01-01T00:00:02.000Z".to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::CustomToolCall {
                id: None,
                status: Some("completed".to_string()),
                call_id: custom_call_id.to_string(),
                name: "js_repl".to_string(),
                input: "console.log('image flow')".to_string(),
            }),
        },
        RolloutLine {
            timestamp: "2024-01-01T00:00:02.500Z".to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::CustomToolCallOutput {
                call_id: custom_call_id.to_string(),
                name: None,
                output: FunctionCallOutputPayload::from_content_items(vec![
                    FunctionCallOutputContentItem::InputImage {
                        image_url: image_url.to_string(),
                        detail: Some(ImageDetail::Original),
                    },
                ]),
            }),
        },
    ];

    let tmpdir = TempDir::new().unwrap();
    let session_path = tmpdir
        .path()
        .join("resume-image-tool-outputs-with-detail.jsonl");
    let mut file = std::fs::File::create(&session_path).unwrap();
    for line in rollout {
        writeln!(file, "{}", serde_json::to_string(&line).unwrap()).unwrap();
    }

    let server = MockServer::start().await;
    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let praxis_home = Arc::new(TempDir::new().unwrap());
    let mut builder = test_praxis().with_model("gpt-5.1");
    let test = builder
        .resume(&server, praxis_home, session_path.clone())
        .await
        .expect("resume conversation");
    test.submit_turn("after resume").await.unwrap();

    let function_output = resp_mock
        .single_request()
        .function_call_output(function_call_id);
    assert_eq!(
        function_output.get("output"),
        Some(&serde_json::json!([
            {
                "type": "input_image",
                "image_url": image_url,
                "detail": "original"
            }
        ]))
    );

    let custom_output = resp_mock
        .single_request()
        .custom_tool_call_output(custom_call_id);
    assert_eq!(
        custom_output.get("output"),
        Some(&serde_json::json!([
            {
                "type": "input_image",
                "image_url": image_url,
                "detail": "original"
            }
        ]))
    );
}

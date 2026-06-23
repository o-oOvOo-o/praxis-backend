#![allow(clippy::expect_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_mid_turn_continuation_compaction() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let context_window = 100;
    let limit = context_window * 90 / 100;
    let over_limit_tokens = context_window * 95 / 100 + 1;

    let first_turn = sse(vec![
        ev_function_call(DUMMY_CALL_ID, DUMMY_FUNCTION_NAME, "{}"),
        ev_completed_with_tokens("r1", over_limit_tokens),
    ]);
    let auto_summary_payload = auto_summary(AUTO_SUMMARY_TEXT);
    let auto_compact_turn = sse(vec![
        ev_assistant_message("m2", &auto_summary_payload),
        ev_completed_with_tokens("r3", /*total_tokens*/ 10),
    ]);
    let post_auto_compact_turn = sse(vec![
        ev_assistant_message("m3", FINAL_REPLY),
        ev_completed_with_tokens("r4", /*total_tokens*/ 10),
    ]);

    // Mount responses in order and keep mocks only for the ones we assert on.
    let first_turn_mock = mount_sse_once(&server, first_turn).await;
    let auto_compact_mock = mount_sse_once(&server, auto_compact_turn).await;
    let post_auto_compact_mock = mount_sse_once(&server, post_auto_compact_turn).await;

    let model_provider = non_openai_model_provider(&server);

    let mut builder = test_praxis().with_config(move |config| {
        config.model_provider = model_provider;
        set_test_compact_prompt(config);
        config.model_context_window = Some(context_window);
        config.model_auto_compact_token_limit = Some(limit);
    });
    let praxis = builder.build(&server).await.unwrap().thread;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: FUNCTION_CALL_LIMIT_MSG.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |msg| matches!(msg, EventMsg::TurnComplete(_))).await;

    // Assert first request captured expected user message that triggers function call.
    let first_request = first_turn_mock.single_request().input();
    assert!(
        first_request.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("message")
                && item
                    .get("content")
                    .and_then(|content| content.as_array())
                    .and_then(|entries| entries.first())
                    .and_then(|entry| entry.get("text"))
                    .and_then(|value| value.as_str())
                    == Some(FUNCTION_CALL_LIMIT_MSG)
        }),
        "first request should include the user message that triggers the function call"
    );

    let function_call_output = auto_compact_mock
        .single_request()
        .function_call_output(DUMMY_CALL_ID);
    let output_text = function_call_output
        .get("output")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    assert!(
        output_text.contains(DUMMY_FUNCTION_NAME),
        "function call output should be sent before auto compact"
    );

    let auto_compact_body = auto_compact_mock.single_request().body_json().to_string();
    assert!(
        body_contains_text(&auto_compact_body, SUMMARIZATION_PROMPT),
        "mid-turn auto compact request should include the summarization prompt after exceeding 95% (limit {limit})"
    );

    insta::assert_snapshot!(
        "mid_turn_compaction_shapes",
        format_labeled_requests_snapshot(
            "True mid-turn continuation compaction after tool output: compact request includes tool artifacts, and the continuation request includes the summary in the same turn.",
            &[
                (
                    "Local Compaction Request",
                    &auto_compact_mock.single_request()
                ),
                (
                    "Local Post-Compaction History Layout",
                    &post_auto_compact_mock.single_request()
                ),
            ]
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_compact_clamps_config_limit_to_context_window() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let context_window = 100;
    let config_limit = 200;
    let over_limit_tokens = context_window * 90 / 100 + 1;

    let first_turn = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed_with_tokens("r1", over_limit_tokens),
    ]);
    let auto_summary_payload = auto_summary(AUTO_SUMMARY_TEXT);
    let auto_compact_turn = sse(vec![
        ev_assistant_message("m2", &auto_summary_payload),
        ev_completed_with_tokens("r2", /*total_tokens*/ 10),
    ]);
    let post_auto_compact_turn = sse(vec![ev_completed_with_tokens(
        "r3", /*total_tokens*/ 10,
    )]);

    let first_turn_mock = mount_sse_once(&server, first_turn).await;
    let auto_compact_mock = mount_sse_once(&server, auto_compact_turn).await;
    mount_sse_once(&server, post_auto_compact_turn).await;

    let model_provider = non_openai_model_provider(&server);
    let mut builder = test_praxis().with_config(move |config| {
        config.model_provider = model_provider;
        set_test_compact_prompt(config);
        config.model_context_window = Some(context_window);
        config.model_auto_compact_token_limit = Some(config_limit);
    });
    let praxis = builder.build(&server).await.unwrap();

    praxis.submit_turn("OVER_LIMIT_TURN").await.unwrap();
    praxis.submit_turn("FOLLOW_UP_AFTER_CLAMP").await.unwrap();

    assert!(
        first_turn_mock.single_request().input().iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("message")
                && item
                    .get("content")
                    .and_then(|content| content.as_array())
                    .and_then(|entries| entries.first())
                    .and_then(|entry| entry.get("text"))
                    .and_then(|value| value.as_str())
                    == Some("OVER_LIMIT_TURN")
        }),
        "first request should contain the over-limit user input"
    );

    let auto_compact_body = auto_compact_mock.single_request().body_json().to_string();
    assert!(
        body_contains_text(&auto_compact_body, SUMMARIZATION_PROMPT),
        "auto compact should run with the summarization prompt when config limit exceeds context"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_compact_counts_encrypted_reasoning_before_last_user() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let first_user = "COUNT_PRE_LAST_REASONING";
    let second_user = "TRIGGER_COMPACT_AT_LIMIT";
    let third_user = "AFTER_REMOTE_COMPACT";

    let pre_last_reasoning_content = "a".repeat(2_400);
    let post_last_reasoning_content = "b".repeat(4_000);

    let first_turn = sse(vec![
        ev_reasoning_item("pre-reasoning", &["pre"], &[&pre_last_reasoning_content]),
        ev_completed_with_tokens("r1", /*total_tokens*/ 10),
    ]);
    let second_turn = sse(vec![
        ev_reasoning_item("post-reasoning", &["post"], &[&post_last_reasoning_content]),
        ev_completed_with_tokens("r2", /*total_tokens*/ 80),
    ]);
    let third_turn = sse(vec![
        ev_assistant_message("m4", FINAL_REPLY),
        ev_completed_with_tokens("r4", /*total_tokens*/ 1),
    ]);

    let request_log = mount_sse_sequence(
        &server,
        vec![
            // Turn 1: reasoning before last user (should count).
            first_turn,
            // Turn 2: reasoning after last user (should be ignored for compaction).
            second_turn,
            // Turn 3: next user turn after remote compaction.
            third_turn,
        ],
    )
    .await;

    let compacted_history = vec![
        praxis_protocol::models::ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![praxis_protocol::models::ContentItem::OutputText {
                text: "REMOTE_COMPACT_SUMMARY".to_string(),
            }],
            end_turn: None,
            phase: None,
        },
        praxis_protocol::models::ResponseItem::Compaction {
            encrypted_content: "ENCRYPTED_COMPACTION_SUMMARY".to_string(),
        },
    ];
    let compact_mock =
        mount_compact_json_once(&server, serde_json::json!({ "output": compacted_history })).await;

    let praxis = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(|config| {
            set_test_compact_prompt(config);
            config.model_auto_compact_token_limit = Some(300);
        })
        .build(&server)
        .await
        .expect("build praxis")
        .thread;

    for (idx, user) in [first_user, second_user, third_user]
        .into_iter()
        .enumerate()
    {
        codex
            .submit(Op::UserInput {
                items: vec![UserInput::Text {
                    text: user.into(),
                    text_elements: Vec::new(),
                }],
                final_output_json_schema: None,
            })
            .await
            .unwrap();
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

        if idx < 2 {
            assert!(
                compact_mock.requests().is_empty(),
                "remote compaction should not run before the next user turn"
            );
        }
    }

    let compact_requests = compact_mock.requests();
    assert_eq!(
        compact_requests.len(),
        1,
        "remote compaction should run once after the second turn"
    );
    assert_eq!(
        compact_requests[0].path(),
        "/v1/responses/compact",
        "remote compaction should hit the compact endpoint"
    );

    let requests = request_log.requests();
    assert_eq!(
        requests.len(),
        3,
        "conversation should include three user turns"
    );
    let second_request_body = requests[1].body_json().to_string();
    assert!(
        !second_request_body.contains("REMOTE_COMPACT_SUMMARY"),
        "second turn should not include compacted history"
    );
    let third_request_body = requests[2].body_json().to_string();
    assert!(
        third_request_body.contains("REMOTE_COMPACT_SUMMARY")
            || third_request_body.contains(FINAL_REPLY),
        "third turn should include compacted history"
    );
    assert!(
        third_request_body.contains("ENCRYPTED_COMPACTION_SUMMARY"),
        "third turn should include compaction summary item"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_compact_runs_when_reasoning_header_clears_between_turns() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let first_user = "SERVER_INCLUDED_FIRST";
    let second_user = "SERVER_INCLUDED_SECOND";
    let third_user = "SERVER_INCLUDED_THIRD";

    let pre_last_reasoning_content = "a".repeat(2_400);
    let post_last_reasoning_content = "b".repeat(4_000);

    let first_turn = sse(vec![
        ev_reasoning_item("pre-reasoning", &["pre"], &[&pre_last_reasoning_content]),
        ev_completed_with_tokens("r1", /*total_tokens*/ 10),
    ]);
    let second_turn = sse(vec![
        ev_reasoning_item("post-reasoning", &["post"], &[&post_last_reasoning_content]),
        ev_completed_with_tokens("r2", /*total_tokens*/ 80),
    ]);
    let third_turn = sse(vec![
        ev_assistant_message("m4", FINAL_REPLY),
        ev_completed_with_tokens("r4", /*total_tokens*/ 1),
    ]);

    let responses = vec![
        sse_response(first_turn).insert_header("X-Reasoning-Included", "true"),
        sse_response(second_turn),
        sse_response(third_turn),
    ];
    mount_response_sequence(&server, responses).await;

    let compacted_history = vec![
        praxis_protocol::models::ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![praxis_protocol::models::ContentItem::OutputText {
                text: "REMOTE_COMPACT_SUMMARY".to_string(),
            }],
            end_turn: None,
            phase: None,
        },
        praxis_protocol::models::ResponseItem::Compaction {
            encrypted_content: "ENCRYPTED_COMPACTION_SUMMARY".to_string(),
        },
    ];
    let compact_mock =
        mount_compact_json_once(&server, serde_json::json!({ "output": compacted_history })).await;

    let praxis = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(|config| {
            set_test_compact_prompt(config);
            config.model_auto_compact_token_limit = Some(300);
        })
        .build(&server)
        .await
        .expect("build praxis")
        .thread;

    for user in [first_user, second_user, third_user] {
        codex
            .submit(Op::UserInput {
                items: vec![UserInput::Text {
                    text: user.into(),
                    text_elements: Vec::new(),
                }],
                final_output_json_schema: None,
            })
            .await
            .unwrap();
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
    }

    let compact_requests = compact_mock.requests();
    assert_eq!(
        compact_requests.len(),
        1,
        "remote compaction should run once after the reasoning header clears"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// TODO(ccunningham): Update once pre-turn compaction includes incoming user input.
async fn snapshot_request_shape_pre_turn_compaction_including_incoming_user_message() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let sse1 = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed_with_tokens("r1", /*total_tokens*/ 60),
    ]);
    let sse2 = sse(vec![
        ev_assistant_message("m2", "SECOND_REPLY"),
        ev_completed_with_tokens("r2", /*total_tokens*/ 500),
    ]);
    let sse3 = sse(vec![
        ev_assistant_message("m3", "PRE_TURN_SUMMARY"),
        ev_completed_with_tokens("r3", /*total_tokens*/ 100),
    ]);
    let sse4 = sse(vec![
        ev_assistant_message("m4", FINAL_REPLY),
        ev_completed_with_tokens("r4", /*total_tokens*/ 80),
    ]);
    let request_log = mount_sse_sequence(&server, vec![sse1, sse2, sse3, sse4]).await;

    let model_provider = non_openai_model_provider(&server);
    let praxis = test_praxis()
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
            config.model_auto_compact_token_limit = Some(200);
        })
        .build(&server)
        .await
        .expect("build praxis")
        .thread;

    for user in ["USER_ONE", "USER_TWO"] {
        codex
            .submit(Op::UserInput {
                items: vec![UserInput::Text {
                    text: user.to_string(),
                    text_elements: Vec::new(),
                }],
                final_output_json_schema: None,
            })
            .await
            .expect("submit user input");
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
    }
    codex
        .submit(Op::OverrideTurnContext {
            cwd: Some(PathBuf::from(PRETURN_CONTEXT_DIFF_CWD)),
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model_provider: None,
            model: None,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await
        .expect("override turn context");
    let image_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAASUVORK5CYII="
        .to_string();
    codex
        .submit(Op::UserInput {
            items: vec![
                UserInput::Image {
                    image_url: image_url.clone(),
                },
                UserInput::Text {
                    text: "USER_THREE".to_string(),
                    text_elements: Vec::new(),
                },
            ],
            final_output_json_schema: None,
        })
        .await
        .expect("submit user input");
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = request_log.requests();
    assert_eq!(requests.len(), 4, "expected user, user, compact, follow-up");

    insta::assert_snapshot!(
        "pre_turn_compaction_including_incoming_shapes",
        format_labeled_requests_snapshot(
            "Pre-turn auto-compaction with a context override emits the context diff in the compact request while the incoming user message is still excluded.",
            &[
                ("Local Compaction Request", &requests[2]),
                ("Local Post-Compaction History Layout", &requests[3]),
            ]
        )
    );
    let compact_request_user_texts = requests[2].message_input_texts("user");
    assert!(
        !compact_request_user_texts
            .iter()
            .any(|text| text == "USER_THREE"),
        "current behavior excludes incoming user message from pre-turn compaction input"
    );
    let follow_up_user_texts = requests[3].message_input_texts("user");
    assert!(
        follow_up_user_texts.iter().any(|text| text == "USER_THREE"),
        "expected post-compaction follow-up request to keep incoming user text"
    );
    let follow_up_user_images = requests[3].message_input_image_urls("user");
    assert!(
        follow_up_user_images
            .iter()
            .any(|url| url == image_url.as_str()),
        "expected post-compaction follow-up request to keep incoming user image content"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// TODO(ccunningham): Update once pre-turn compaction context-overflow handling includes incoming
// user input and emits richer oversized-input messaging.
async fn snapshot_request_shape_pre_turn_compaction_strips_incoming_model_switch() {
    skip_if_no_network!();

    let server = start_mock_server().await;
    let previous_model = "gpt-5.1-codex-max";
    let next_model = "gpt-5.2-codex";

    let request_log = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_assistant_message("m1", "BEFORE_SWITCH_REPLY"),
                ev_completed_with_tokens("r1", /*total_tokens*/ 500),
            ]),
            sse(vec![
                ev_assistant_message("m2", "PRETURN_SWITCH_SUMMARY"),
                ev_completed_with_tokens("r2", /*total_tokens*/ 100),
            ]),
            sse(vec![
                ev_assistant_message("m3", "AFTER_SWITCH_REPLY"),
                ev_completed_with_tokens("r3", /*total_tokens*/ 100),
            ]),
        ],
    )
    .await;

    let model_provider = non_openai_model_provider(&server);
    let test = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_model(previous_model)
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
            let _ = config.features.enable(Feature::RemoteModels);
            config.model_auto_compact_token_limit = Some(200);
        })
        .build(&server)
        .await
        .expect("build praxis");

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "BEFORE_SWITCH_USER".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: previous_model.to_string(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await
        .expect("submit first user turn");
    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "AFTER_SWITCH_USER".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: next_model.to_string(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await
        .expect("submit second user turn");
    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = request_log.requests();
    assert_eq!(
        requests.len(),
        3,
        "expected first turn, pre-turn compact, and post-compact follow-up requests"
    );

    let compact_body = requests[1].body_json().to_string();
    assert!(
        body_contains_text(&compact_body, SUMMARIZATION_PROMPT),
        "pre-turn compaction request should include summarization prompt"
    );
    assert!(
        !compact_body.contains("<model_switch>"),
        "pre-turn compaction request should strip incoming model-switch update item"
    );

    let follow_up_body = requests[2].body_json().to_string();
    assert!(
        follow_up_body.contains("<model_switch>"),
        "post-compaction follow-up should include model-switch update item"
    );

    insta::assert_snapshot!(
        "pre_turn_compaction_strips_incoming_model_switch_shapes",
        format_labeled_requests_snapshot(
            "Pre-turn compaction during model switch (without pre-sampling model-switch compaction): current behavior strips incoming <model_switch> from the compact request and restores it in the post-compaction follow-up request.",
            &[
                ("Initial Request (Previous Model)", &requests[0]),
                ("Local Compaction Request", &requests[1]),
                ("Local Post-Compaction History Layout", &requests[2]),
            ]
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_pre_turn_compaction_context_window_exceeded() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let first_turn = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed_with_tokens("r1", /*total_tokens*/ 500),
    ]);
    let mut responses = vec![first_turn];
    responses.extend(
        (0..5).map(|_| {
            sse_failed(
                "compact-failed",
                "context_length_exceeded",
                "Your input exceeds the context window of this model. Please adjust your input and try again.",
            )
        }),
    );
    let request_log = mount_sse_sequence(&server, responses).await;

    let mut model_provider = non_openai_model_provider(&server);
    model_provider.stream_max_retries = Some(0);
    let praxis = test_praxis()
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
            config.model_auto_compact_token_limit = Some(200);
        })
        .build(&server)
        .await
        .expect("build praxis")
        .thread;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await
        .expect("submit first user");
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "USER_TWO".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await
        .expect("submit second user");
    let error_message = wait_for_event_match(&praxis, |event| match event {
        EventMsg::Error(err) => Some(err.message.clone()),
        _ => None,
    })
    .await;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = request_log.requests();
    assert!(
        requests.len() >= 2,
        "expected first turn and at least one compaction request"
    );

    insta::assert_snapshot!(
        "pre_turn_compaction_context_window_exceeded_shapes",
        format_labeled_requests_snapshot(
            "Pre-turn auto-compaction context-window failure: compaction request excludes the incoming user message and the turn errors.",
            &[(
                "Local Compaction Request (Incoming User Excluded)",
                &requests[1]
            ),]
        )
    );

    assert!(
        error_message.contains("ran out of room in the model's context window"),
        "expected context window exceeded message, got {error_message}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_manual_compact_without_previous_user_messages() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let compact_turn = sse(vec![
        ev_assistant_message("m1", "MANUAL_EMPTY_SUMMARY"),
        ev_completed_with_tokens("r1", /*total_tokens*/ 90),
    ]);
    let follow_up_turn = sse(vec![
        ev_assistant_message("m2", FINAL_REPLY),
        ev_completed_with_tokens("r2", /*total_tokens*/ 80),
    ]);
    let request_log = mount_sse_sequence(&server, vec![compact_turn, follow_up_turn]).await;

    let model_provider = non_openai_model_provider(&server);
    let praxis = test_praxis()
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
        })
        .build(&server)
        .await
        .expect("build praxis")
        .thread;

    praxis.submit(Op::Compact).await.expect("run /compact");
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "AFTER_MANUAL_EMPTY_COMPACT".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await
        .expect("submit follow-up user input");
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = request_log.requests();
    assert_eq!(
        requests.len(),
        2,
        "expected manual /compact request and follow-up turn request"
    );

    insta::assert_snapshot!(
        "manual_compact_without_prev_user_shapes",
        format_labeled_requests_snapshot(
            "Manual /compact with no prior user turn currently still issues a compaction request; follow-up turn carries canonical context and the new user message.",
            &[
                ("Local Compaction Request", &requests[0]),
                ("Local Post-Compaction History Layout", &requests[1]),
            ]
        )
    );
}

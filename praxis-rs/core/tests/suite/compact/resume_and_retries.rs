#![allow(clippy::expect_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_compact_runs_after_resume_when_token_usage_is_over_limit() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let limit = 200_000;
    let over_limit_tokens = 250_000;
    let remote_summary = "REMOTE_COMPACT_SUMMARY";

    let compacted_history = vec![
        praxis_protocol::models::ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![praxis_protocol::models::ContentItem::OutputText {
                text: remote_summary.to_string(),
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

    let mut builder = test_praxis().with_config(move |config| {
        set_test_compact_prompt(config);
        config.model_auto_compact_token_limit = Some(limit);
    });
    let initial = builder.build(&server).await.unwrap();
    let home = initial.home.clone();
    let rollout_path = initial
        .session_configured
        .rollout_path
        .clone()
        .expect("rollout path");

    // A single over-limit completion should not auto-compact until the next user message.
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("m1", FIRST_REPLY),
            ev_completed_with_tokens("r1", over_limit_tokens),
        ]),
    )
    .await;
    initial.submit_turn("OVER_LIMIT_TURN").await.unwrap();

    assert!(
        compact_mock.requests().is_empty(),
        "remote compaction should not run before the next user message"
    );

    let mut resume_builder = test_praxis().with_config(move |config| {
        set_test_compact_prompt(config);
        config.model_auto_compact_token_limit = Some(limit);
    });
    let resumed = resume_builder
        .resume(&server, home, rollout_path)
        .await
        .unwrap();

    let follow_up_user = "AFTER_RESUME_USER";
    let sse_follow_up = sse(vec![
        ev_assistant_message("m2", FINAL_REPLY),
        ev_completed("r2"),
    ]);

    let follow_up_matcher = move |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body.contains(follow_up_user) && body.contains(remote_summary)
    };
    mount_sse_once_match(&server, follow_up_matcher, sse_follow_up).await;

    resumed
        .thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: follow_up_user.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: resumed.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: resumed.session_configured.model.clone(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await
        .unwrap();

    wait_for_event(&resumed.thread, |event| {
        matches!(event, EventMsg::ContextCompacted(_))
    })
    .await;
    wait_for_event(&resumed.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let compact_requests = compact_mock.requests();
    assert_eq!(
        compact_requests.len(),
        1,
        "remote compaction should run once after resume"
    );
    assert_eq!(
        compact_requests[0].path(),
        "/v1/responses/compact",
        "remote compaction should hit the compact endpoint"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_sampling_compact_runs_on_switch_to_smaller_context_model() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    let previous_model = "gpt-5.2-codex";
    let next_model = "gpt-5.1-codex-max";

    let models_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: vec![
                model_info_with_context_window(previous_model, /*context_window*/ 273_000),
                model_info_with_context_window(next_model, /*context_window*/ 125_000),
            ],
        },
    )
    .await;

    let request_log = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_assistant_message("m1", "before switch"),
                ev_completed_with_tokens("r1", /*total_tokens*/ 120_000),
            ]),
            sse(vec![
                ev_assistant_message("m2", "PRE_SAMPLING_SUMMARY"),
                ev_completed_with_tokens("r2", /*total_tokens*/ 10),
            ]),
            sse(vec![
                ev_assistant_message("m3", "after switch"),
                ev_completed_with_tokens("r3", /*total_tokens*/ 100),
            ]),
        ],
    )
    .await;

    let model_provider = non_openai_model_provider(&server);
    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_model(previous_model)
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
        });
    let test = builder.build(&server).await.expect("build test praxis");

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "before switch".into(),
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
                text: "after switch".into(),
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
    assert_compaction_uses_turn_lifecycle_id(&test.thread).await;

    let requests = request_log.requests();
    assert_eq!(models_mock.requests().len(), 1);
    assert_eq!(
        requests.len(),
        3,
        "expected user, compact, and follow-up requests"
    );
    assert_pre_sampling_switch_compaction_requests(
        &requests[0].body_json(),
        &requests[1].body_json(),
        &requests[2].body_json(),
        previous_model,
        next_model,
    );

    insta::assert_snapshot!(
        "pre_sampling_model_switch_compaction_shapes",
        format_labeled_requests_snapshot(
            "Pre-sampling compaction on model switch to a smaller context window: current behavior compacts using prior-turn history only (incoming user message excluded), and the follow-up request carries compacted history plus the new user message.",
            &[
                ("Initial Request (Previous Model)", &requests[0]),
                ("Pre-sampling Compaction Request", &requests[1]),
                (
                    "Post-Compaction Follow-up Request (Next Model)",
                    &requests[2]
                ),
            ]
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_sampling_compact_runs_after_resume_and_switch_to_smaller_model() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    let previous_model = "gpt-5.2-codex";
    let next_model = "gpt-5.1-codex-max";

    let models_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: vec![
                model_info_with_context_window(previous_model, /*context_window*/ 273_000),
                model_info_with_context_window(next_model, /*context_window*/ 125_000),
            ],
        },
    )
    .await;

    let request_log = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_assistant_message("m1", "before resume"),
                ev_completed_with_tokens("r1", /*total_tokens*/ 120_000),
            ]),
            sse(vec![
                ev_assistant_message("m2", "PRE_SAMPLING_SUMMARY"),
                ev_completed_with_tokens("r2", /*total_tokens*/ 10),
            ]),
            sse(vec![
                ev_assistant_message("m3", "after resume"),
                ev_completed_with_tokens("r3", /*total_tokens*/ 100),
            ]),
        ],
    )
    .await;

    let model_provider = non_openai_model_provider(&server);
    let mut initial_builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_model(previous_model)
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
        });
    let initial = initial_builder
        .build(&server)
        .await
        .expect("build initial test codex");
    let home = initial.home.clone();
    let rollout_path = initial
        .session_configured
        .rollout_path
        .clone()
        .expect("rollout path");

    initial
        .thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "before resume".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: initial.cwd.path().to_path_buf(),
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
        .expect("submit pre-resume turn");
    wait_for_event(&initial.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    initial
        .thread
        .submit(Op::Shutdown)
        .await
        .expect("shutdown initial session");
    wait_for_event(&initial.thread, |event| {
        matches!(event, EventMsg::ShutdownComplete)
    })
    .await;

    let model_provider = non_openai_model_provider(&server);
    let mut resumed_builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_model(previous_model)
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
        });
    let resumed = resumed_builder
        .resume(&server, home, rollout_path)
        .await
        .expect("resume praxis");

    resumed
        .thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "after resume".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: resumed.cwd.path().to_path_buf(),
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
        .expect("submit resumed user turn");
    assert_compaction_uses_turn_lifecycle_id(&resumed.thread).await;

    let requests = request_log.requests();
    assert_eq!(models_mock.requests().len(), 1);
    assert_eq!(
        requests.len(),
        3,
        "expected user, compact, and follow-up requests"
    );
    assert_pre_sampling_switch_compaction_requests(
        &requests[0].body_json(),
        &requests[1].body_json(),
        &requests[2].body_json(),
        previous_model,
        next_model,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_compact_persists_rollout_entries() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let sse1 = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed_with_tokens("r1", /*total_tokens*/ 70_000),
    ]);

    let sse2 = sse(vec![
        ev_assistant_message("m2", "SECOND_REPLY"),
        ev_completed_with_tokens("r2", /*total_tokens*/ 330_000),
    ]);

    let auto_summary_payload = auto_summary(AUTO_SUMMARY_TEXT);
    let sse3 = sse(vec![
        ev_assistant_message("m3", &auto_summary_payload),
        ev_completed_with_tokens("r3", /*total_tokens*/ 200),
    ]);
    let sse4 = sse(vec![
        ev_assistant_message("m4", FINAL_REPLY),
        ev_completed_with_tokens("r4", /*total_tokens*/ 120),
    ]);

    let first_matcher = |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body.contains(FIRST_AUTO_MSG)
            && !body.contains(SECOND_AUTO_MSG)
            && !body_contains_text(body, SUMMARIZATION_PROMPT)
    };
    mount_sse_once_match(&server, first_matcher, sse1).await;

    let second_matcher = |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body.contains(SECOND_AUTO_MSG)
            && body.contains(FIRST_AUTO_MSG)
            && !body_contains_text(body, SUMMARIZATION_PROMPT)
    };
    mount_sse_once_match(&server, second_matcher, sse2).await;

    let third_matcher = |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body_contains_text(body, SUMMARIZATION_PROMPT)
    };
    mount_sse_once_match(&server, third_matcher, sse3).await;

    let fourth_matcher = |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body.contains(POST_AUTO_USER_MSG) && !body_contains_text(body, SUMMARIZATION_PROMPT)
    };
    mount_sse_once_match(&server, fourth_matcher, sse4).await;

    let model_provider = non_openai_model_provider(&server);

    let mut builder = test_praxis().with_config(move |config| {
        config.model_provider = model_provider;
        set_test_compact_prompt(config);
        config.model_auto_compact_token_limit = Some(200_000);
    });
    let test = builder.build(&server).await.unwrap();
    let praxis = test.thread.clone();
    let session_configured = test.session_configured;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: FIRST_AUTO_MSG.into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: SECOND_AUTO_MSG.into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: POST_AUTO_USER_MSG.into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    praxis.submit(Op::Shutdown).await.unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::ShutdownComplete)).await;

    let rollout_path = session_configured.rollout_path.expect("rollout path");
    let text = std::fs::read_to_string(&rollout_path).unwrap_or_else(|e| {
        panic!(
            "failed to read rollout file {}: {e}",
            rollout_path.display()
        )
    });

    let mut turn_context_count = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry): Result<RolloutLine, _> = serde_json::from_str(trimmed) else {
            continue;
        };
        match entry.item {
            RolloutItem::TurnContext(_) => {
                turn_context_count += 1;
            }
            RolloutItem::Compacted(_) => {}
            _ => {}
        }
    }

    assert_eq!(
        turn_context_count, 3,
        "rollout should contain one TurnContext entry per real user turn"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_compact_retries_after_context_window_error() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let user_turn = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed("r1"),
    ]);
    let compact_failed = sse_failed(
        "resp-fail",
        "context_length_exceeded",
        CONTEXT_LIMIT_MESSAGE,
    );
    let compact_succeeds = sse(vec![
        ev_assistant_message("m2", SUMMARY_TEXT),
        ev_completed("r2"),
    ]);

    let request_log = mount_sse_sequence(
        &server,
        vec![
            user_turn.clone(),
            compact_failed.clone(),
            compact_succeeds.clone(),
        ],
    )
    .await;

    let model_provider = non_openai_model_provider(&server);

    let mut builder = test_praxis().with_config(move |config| {
        config.model_provider = model_provider;
        set_test_compact_prompt(config);
        config.model_auto_compact_token_limit = Some(200_000);
    });
    let praxis = builder.build(&server).await.unwrap().thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "first turn".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    praxis.submit(Op::Compact).await.unwrap();
    let EventMsg::BackgroundEvent(event) =
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::BackgroundEvent(_))).await
    else {
        panic!("expected background event after compact retry");
    };
    assert!(
        event.message.contains("Trimmed 1 older thread item"),
        "background event should mention trimmed item count: {}",
        event.message
    );
    let warning_event = wait_for_event(&praxis, |ev| matches!(ev, EventMsg::Warning(_))).await;
    let EventMsg::Warning(WarningEvent { message }) = warning_event else {
        panic!("expected warning event after compact retry");
    };
    assert_eq!(message, COMPACT_WARNING_MESSAGE);
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = request_log.requests();
    assert_eq!(
        requests.len(),
        3,
        "expected user turn and two compact attempts"
    );

    let compact_attempt = requests[1].body_json();
    let retry_attempt = requests[2].body_json();

    let compact_input = compact_attempt["input"]
        .as_array()
        .unwrap_or_else(|| panic!("compact attempt missing input array: {compact_attempt}"));
    let retry_input = retry_attempt["input"]
        .as_array()
        .unwrap_or_else(|| panic!("retry attempt missing input array: {retry_attempt}"));
    let compact_contains_prompt =
        body_contains_text(&compact_attempt.to_string(), SUMMARIZATION_PROMPT);
    let retry_contains_prompt =
        body_contains_text(&retry_attempt.to_string(), SUMMARIZATION_PROMPT);
    assert_eq!(
        compact_contains_prompt, retry_contains_prompt,
        "compact attempts should consistently include or omit the summarization prompt"
    );
    assert_eq!(
        retry_input.len(),
        compact_input.len().saturating_sub(1),
        "retry should drop exactly one history item (before {} vs after {})",
        compact_input.len(),
        retry_input.len()
    );
    if let (Some(first_before), Some(first_after)) = (compact_input.first(), retry_input.first()) {
        assert_ne!(
            first_before, first_after,
            "retry should drop the oldest conversation item"
        );
    } else {
        panic!("expected non-empty compact inputs");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// TODO(ccunningham): Re-enable after the follow-up compaction behavior PR lands.
// Current main behavior around non-context manual /compact failures is known-incorrect.
#[ignore = "behavior change covered in follow-up compaction PR"]
async fn manual_compact_non_context_failure_retries_then_emits_task_error() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let user_turn = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed("r1"),
    ]);
    let compact_failed_1 = sse_failed(
        "resp-fail-1",
        "server_error",
        "temporary compact failure one",
    );
    let compact_failed_2 = sse_failed(
        "resp-fail-2",
        "server_error",
        "temporary compact failure two",
    );

    mount_sse_sequence(&server, vec![user_turn, compact_failed_1, compact_failed_2]).await;

    let mut model_provider = non_openai_model_provider(&server);
    model_provider.stream_max_retries = Some(1);

    let praxis = test_praxis()
        .with_config(move |config| {
            config.model_provider = model_provider;
            set_test_compact_prompt(config);
            config.model_auto_compact_token_limit = Some(200_000);
        })
        .build(&server)
        .await
        .expect("build praxis")
        .thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "first turn".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .expect("submit user input");
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    praxis.submit(Op::Compact).await.expect("trigger compact");

    let reconnect_message = wait_for_event_match(&praxis, |event| match event {
        EventMsg::StreamError(stream_error) => Some(stream_error.message.clone()),
        _ => None,
    })
    .await;
    assert!(
        reconnect_message.contains("Reconnecting... 1/1"),
        "expected reconnect stream error message, got {reconnect_message}"
    );

    let task_error_message = wait_for_event_match(&praxis, |event| match event {
        EventMsg::Error(err) => Some(err.message.clone()),
        _ => None,
    })
    .await;
    assert!(
        task_error_message.contains("Error running local compact task"),
        "expected local compact task error prefix, got {task_error_message}"
    );
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
}

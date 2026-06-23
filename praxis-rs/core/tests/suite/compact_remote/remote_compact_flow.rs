#![allow(clippy::expect_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_compact_replaces_history_for_followups() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let harness = TestPraxisHarness::with_builder(
        test_praxis().with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing()),
    )
    .await?;
    let praxis = harness.test().thread.clone();
    let session_id = harness.test().session_configured.session_id.to_string();

    let responses_mock = responses::mount_sse_sequence(
        harness.server(),
        vec![
            responses::sse(vec![
                responses::ev_assistant_message("m1", "FIRST_REMOTE_REPLY"),
                responses::ev_completed("resp-1"),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m2", "AFTER_COMPACT_REPLY"),
                responses::ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let compacted_history = vec![ResponseItem::Compaction {
        encrypted_content: "ENCRYPTED_COMPACTION_SUMMARY".to_string(),
    }];
    let compact_mock = responses::mount_compact_json_once(
        harness.server(),
        serde_json::json!({ "output": compacted_history.clone() }),
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hello remote compact".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    praxis.submit(Op::Compact).await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "after compact".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let compact_request = compact_mock.single_request();
    assert_eq!(compact_request.path(), "/v1/responses/compact");
    assert_eq!(
        compact_request.header("chatgpt-account-id").as_deref(),
        Some("account_id")
    );
    assert_eq!(
        compact_request.header("authorization").as_deref(),
        Some("Bearer Access Token")
    );
    assert_eq!(
        compact_request.header("session_id").as_deref(),
        Some(session_id.as_str())
    );
    let compact_body = compact_request.body_json();
    assert_eq!(
        compact_body.get("model").and_then(|v| v.as_str()),
        Some(harness.test().session_configured.model.as_str())
    );
    let response_requests = responses_mock.requests();
    let first_response_request = response_requests.first().expect("initial request missing");
    assert_eq!(
        compact_body["tools"],
        first_response_request.body_json()["tools"],
        "compact requests should send the same tools payload as /v1/responses"
    );
    assert_eq!(
        compact_body["parallel_tool_calls"],
        first_response_request.body_json()["parallel_tool_calls"],
        "compact requests should match /v1/responses parallel_tool_calls"
    );
    assert_eq!(
        compact_body["reasoning"],
        first_response_request.body_json()["reasoning"],
        "compact requests should match /v1/responses reasoning"
    );
    assert_eq!(
        compact_body["text"],
        first_response_request.body_json()["text"],
        "compact requests should match /v1/responses text controls"
    );
    let compact_body_text = compact_body.to_string();
    assert!(
        compact_body_text.contains("hello remote compact"),
        "expected compact request to include user history"
    );
    assert!(
        compact_body_text.contains("FIRST_REMOTE_REPLY"),
        "expected compact request to include assistant history"
    );

    let response_requests = responses_mock.requests();
    let follow_up_request = response_requests.last().expect("follow-up request missing");
    let follow_up_body = follow_up_request.body_json().to_string();
    assert!(
        follow_up_body.contains("\"type\":\"compaction\""),
        "expected follow-up request to use compacted history"
    );
    assert!(
        follow_up_body.contains("ENCRYPTED_COMPACTION_SUMMARY"),
        "expected follow-up request to include compaction summary item"
    );
    assert!(
        !follow_up_body.contains("FIRST_REMOTE_REPLY"),
        "expected follow-up request to drop pre-compaction assistant messages"
    );
    assert!(
        !follow_up_body.contains("hello remote compact"),
        "expected follow-up request to drop compacted-away user turns when remote output omits them"
    );

    insta::assert_snapshot!(
        "remote_manual_compact_with_history_shapes",
        format_labeled_requests_snapshot(
            "Remote manual /compact where remote compact output is compaction-only: follow-up layout uses the returned compaction item plus new user message.",
            &[
                ("Remote Compaction Request", &compact_request),
                ("Remote Post-Compaction History Layout", follow_up_request),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_compact_runs_automatically() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let harness = TestPraxisHarness::with_builder(
        test_praxis().with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing()),
    )
    .await?;
    let praxis = harness.test().thread.clone();
    let session_id = harness.test().session_configured.session_id.to_string();

    mount_sse_once(
        harness.server(),
        sse(vec![
            responses::ev_shell_command_call("m1", "echo 'hi'"),
            responses::ev_completed_with_tokens("resp-1", /*total_tokens*/ 100000000), // over token limit
        ]),
    )
    .await;
    let responses_mock = mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_assistant_message("m2", "AFTER_COMPACT_REPLY"),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    let compact_mock = responses::mount_compact_user_history_with_summary_once(
        harness.server(),
        "REMOTE_COMPACTED_SUMMARY",
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hello remote compact".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;

    let message = wait_for_event_match(&praxis, |event| match event {
        EventMsg::ContextCompacted(_) => Some(true),
        _ => None,
    })
    .await;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;
    assert!(message);
    assert_eq!(compact_mock.requests().len(), 1);
    assert_eq!(
        compact_mock
            .single_request()
            .header("session_id")
            .as_deref(),
        Some(session_id.as_str())
    );
    let follow_up_request = responses_mock.single_request();
    let follow_up_body = follow_up_request.body_json().to_string();
    assert!(follow_up_body.contains("REMOTE_COMPACTED_SUMMARY"));

    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_compact_trims_function_call_history_to_fit_context_window() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let first_user_message = "turn with retained shell call";
    let second_user_message = "turn with trimmed shell call";
    let retained_call_id = "retained-call";
    let trimmed_call_id = "trimmed-call";
    let retained_command = "echo retained-shell-output";
    let trimmed_command = "yes x | head -n 3000";

    let harness = TestPraxisHarness::with_builder(
        test_praxis()
            .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
            .with_config(|config| {
                config.model_context_window = Some(2_000);
                config.model_auto_compact_token_limit = Some(200_000);
            }),
    )
    .await?;
    let praxis = harness.test().thread.clone();

    responses::mount_sse_sequence(
        harness.server(),
        vec![
            sse(vec![
                responses::ev_shell_command_call(retained_call_id, retained_command),
                responses::ev_completed("retained-call-response"),
            ]),
            sse(vec![
                responses::ev_assistant_message("retained-assistant", "retained complete"),
                responses::ev_completed("retained-final-response"),
            ]),
            sse(vec![
                responses::ev_shell_command_call(trimmed_call_id, trimmed_command),
                responses::ev_completed("trimmed-call-response"),
            ]),
        ],
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: first_user_message.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: second_user_message.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let compact_mock = responses::mount_compact_user_history_with_summary_once(
        harness.server(),
        "REMOTE_COMPACT_SUMMARY",
    )
    .await;

    praxis.submit(Op::Compact).await?;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let compact_request = compact_mock.single_request();
    let user_messages = compact_request.message_input_texts("user");
    assert!(
        user_messages
            .iter()
            .any(|message| message == first_user_message),
        "expected compact request to retain earlier user history"
    );
    assert!(
        user_messages
            .iter()
            .any(|message| message == second_user_message),
        "expected compact request to retain the user boundary message"
    );

    assert!(
        compact_request.has_function_call(retained_call_id)
            && compact_request
                .function_call_output_text(retained_call_id)
                .is_some(),
        "expected compact request to keep the older function call/result pair"
    );
    assert!(
        !compact_request.has_function_call(trimmed_call_id)
            && compact_request
                .function_call_output_text(trimmed_call_id)
                .is_none(),
        "expected compact request to drop the trailing function call/result pair past the boundary"
    );

    assert_eq!(
        compact_request.inputs_of_type("function_call").len(),
        1,
        "expected exactly one function call after trimming"
    );
    assert_eq!(
        compact_request.inputs_of_type("function_call_output").len(),
        1,
        "expected exactly one function call output after trimming"
    );

    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_remote_compact_trims_function_call_history_to_fit_context_window() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let first_user_message = "turn with retained shell call";
    let second_user_message = "turn with trimmed shell call";
    let retained_call_id = "retained-call";
    let trimmed_call_id = "trimmed-call";
    let retained_command = "echo retained-shell-output";
    let trimmed_command = "yes x | head -n 3000";
    let harness = TestPraxisHarness::with_builder(
        test_praxis()
            .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
            .with_config(|config| {
                config.model_context_window = Some(2_000);
                config.model_auto_compact_token_limit = Some(200_000);
            }),
    )
    .await?;
    let praxis = harness.test().thread.clone();

    responses::mount_sse_sequence(
        harness.server(),
        vec![
            sse(vec![
                responses::ev_shell_command_call(retained_call_id, retained_command),
                responses::ev_completed_with_tokens(
                    "retained-call-response",
                    /*total_tokens*/ 100,
                ),
            ]),
            sse(vec![
                responses::ev_assistant_message("retained-assistant", "retained complete"),
                responses::ev_completed("retained-final-response"),
            ]),
            sse(vec![
                responses::ev_shell_command_call(trimmed_call_id, trimmed_command),
                responses::ev_completed_with_tokens(
                    "trimmed-call-response",
                    /*total_tokens*/ 100,
                ),
            ]),
            sse(vec![responses::ev_completed_with_tokens(
                "trimmed-final-response",
                /*total_tokens*/ 500_000,
            )]),
        ],
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: first_user_message.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: second_user_message.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let compact_mock = responses::mount_compact_user_history_with_summary_once(
        harness.server(),
        "REMOTE_AUTO_COMPACT_SUMMARY",
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "turn that triggers auto compact".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;
    assert_eq!(
        compact_mock.requests().len(),
        1,
        "expected exactly one remote compact request"
    );

    let compact_request = compact_mock.single_request();
    let user_messages = compact_request.message_input_texts("user");
    assert!(
        user_messages
            .iter()
            .any(|message| message == first_user_message),
        "expected compact request to retain earlier user history"
    );
    assert!(
        user_messages
            .iter()
            .any(|message| message == second_user_message),
        "expected compact request to retain the user boundary message"
    );

    assert!(
        compact_request.has_function_call(retained_call_id)
            && compact_request
                .function_call_output_text(retained_call_id)
                .is_some(),
        "expected compact request to keep the older function call/result pair"
    );
    assert!(
        !compact_request.has_function_call(trimmed_call_id)
            && compact_request
                .function_call_output_text(trimmed_call_id)
                .is_none(),
        "expected compact request to drop the trailing function call/result pair past the boundary"
    );

    assert_eq!(
        compact_request.inputs_of_type("function_call").len(),
        1,
        "expected exactly one function call after trimming"
    );
    assert_eq!(
        compact_request.inputs_of_type("function_call_output").len(),
        1,
        "expected exactly one function call output after trimming"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_remote_compact_failure_stops_agent_loop() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let harness = TestPraxisHarness::with_builder(
        test_praxis()
            .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
            .with_config(|config| {
                config.model_auto_compact_token_limit = Some(120);
            }),
    )
    .await?;
    let praxis = harness.test().thread.clone();

    mount_sse_once(
        harness.server(),
        sse(vec![
            responses::ev_assistant_message("initial-assistant", "initial turn complete"),
            responses::ev_completed_with_tokens("initial-response", /*total_tokens*/ 500_000),
        ]),
    )
    .await;

    let first_compact_mock = responses::mount_compact_json_once(
        harness.server(),
        serde_json::json!({ "output": "invalid compact payload shape" }),
    )
    .await;
    let post_compact_turn_mock = mount_sse_once(
        harness.server(),
        sse(vec![
            responses::ev_assistant_message("post-compact-assistant", "should not run"),
            responses::ev_completed("post-compact-response"),
        ]),
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "turn that exceeds token threshold".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "turn that triggers auto compact".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;

    let error_message = wait_for_event_match(&praxis, |event| match event {
        EventMsg::Error(err) => Some(err.message.clone()),
        _ => None,
    })
    .await;
    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    assert!(
        error_message.contains("Error running remote compact task"),
        "expected remote compact task error prefix, got {error_message}"
    );
    assert_eq!(
        first_compact_mock.requests().len(),
        1,
        "expected first remote compact attempt with incoming items"
    );
    assert!(
        post_compact_turn_mock.requests().is_empty(),
        "expected agent loop to stop after compaction failure"
    );

    insta::assert_snapshot!(
        "remote_pre_turn_compaction_failure_shapes",
        format_labeled_requests_snapshot(
            "Remote pre-turn auto-compaction parse failure: compaction request excludes the incoming user message and the turn stops.",
            &[(
                "Remote Compaction Request (Incoming User Excluded)",
                &first_compact_mock.single_request()
            ),]
        )
    );

    Ok(())
}

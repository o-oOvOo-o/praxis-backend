#![allow(clippy::expect_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// TODO(ccunningham): Update once remote pre-turn compaction includes incoming user input.
async fn snapshot_request_shape_remote_pre_turn_compaction_including_incoming_user_message()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let harness = TestPraxisHarness::with_builder(
        test_praxis()
            .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
            .with_config(|config| {
                config.model_auto_compact_token_limit = Some(200);
            }),
    )
    .await?;
    let praxis = harness.test().thread.clone();

    let responses_mock = responses::mount_sse_sequence(
        harness.server(),
        vec![
            responses::sse(vec![
                responses::ev_assistant_message("m1", "REMOTE_FIRST_REPLY"),
                responses::ev_completed_with_tokens("r1", /*total_tokens*/ 60),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m2", "REMOTE_SECOND_REPLY"),
                responses::ev_completed_with_tokens("r2", /*total_tokens*/ 500),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m3", "REMOTE_FINAL_REPLY"),
                responses::ev_completed_with_tokens("r3", /*total_tokens*/ 80),
            ]),
        ],
    )
    .await;

    let compact_mock = responses::mount_compact_user_history_with_summary_once(
        harness.server(),
        &summary_with_prefix("REMOTE_PRE_TURN_SUMMARY"),
    )
    .await;

    for user in ["USER_ONE", "USER_TWO", "USER_THREE"] {
        if user == "USER_THREE" {
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
                .await?;
        }
        codex
            .submit(Op::UserInput {
                items: vec![UserInput::Text {
                    text: user.to_string(),
                    text_elements: Vec::new(),
                }],
                final_output_json_schema: None,
            })
            .await?;
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
    }

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(
        requests.len(),
        3,
        "expected user, user, and post-compact turn"
    );

    let compact_request = compact_mock.single_request();
    insta::assert_snapshot!(
        "remote_pre_turn_compaction_including_incoming_shapes",
        format_labeled_requests_snapshot(
            "Remote pre-turn auto-compaction with a context override emits the context diff in the compact request while excluding the incoming user message.",
            &[
                ("Remote Compaction Request", &compact_request),
                ("Remote Post-Compaction History Layout", &requests[2]),
            ]
        )
    );
    assert_eq!(
        requests[2]
            .message_input_texts("user")
            .iter()
            .filter(|text| text.as_str() == "USER_THREE")
            .count(),
        1,
        "post-compaction request should contain incoming user exactly once from runtime append"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_pre_turn_compaction_strips_incoming_model_switch()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let previous_model = "gpt-5.1-codex-max";
    let next_model = "gpt-5.2-codex";
    let harness = TestPraxisHarness::with_builder(
        test_praxis()
            .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
            .with_model(previous_model)
            .with_config(|config| {
                config.model_auto_compact_token_limit = Some(200);
            }),
    )
    .await?;
    let praxis = harness.test().thread.clone();

    let initial_turn_request_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_assistant_message("m1", "BEFORE_SWITCH_REPLY"),
            responses::ev_completed_with_tokens("r1", /*total_tokens*/ 500),
        ]),
    )
    .await;
    let post_compact_turn_request_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_assistant_message("m2", "AFTER_SWITCH_REPLY"),
            responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
        ]),
    )
    .await;
    let compact_mock = responses::mount_compact_user_history_with_summary_once(
        harness.server(),
        &summary_with_prefix("REMOTE_SWITCH_SUMMARY"),
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "BEFORE_SWITCH_USER".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model_provider: None,
            model: Some(next_model.to_string()),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "AFTER_SWITCH_USER".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(
        compact_mock.requests().len(),
        1,
        "expected a single remote pre-turn compaction request"
    );
    assert_eq!(
        initial_turn_request_mock.requests().len(),
        1,
        "expected initial turn request"
    );
    assert_eq!(
        post_compact_turn_request_mock.requests().len(),
        1,
        "expected post-compaction follow-up request"
    );

    let initial_turn_request = initial_turn_request_mock.single_request();
    let compact_request = compact_mock.single_request();
    let post_compact_turn_request = post_compact_turn_request_mock.single_request();
    let compact_body = compact_request.body_json().to_string();
    assert!(
        !compact_body.contains("AFTER_SWITCH_USER"),
        "current behavior excludes incoming user from the pre-turn remote compaction request"
    );
    assert!(
        !compact_body.contains("<model_switch>"),
        "pre-turn remote compaction request should strip incoming model-switch update item"
    );

    let follow_up_body = post_compact_turn_request.body_json().to_string();
    assert!(
        follow_up_body.contains("BEFORE_SWITCH_USER"),
        "post-compaction follow-up should preserve older user messages when they fit"
    );
    assert!(
        follow_up_body.contains("AFTER_SWITCH_USER"),
        "post-compaction follow-up should preserve incoming user message via runtime append"
    );
    assert!(
        follow_up_body.contains("<model_switch>"),
        "post-compaction follow-up should include the model-switch update item"
    );

    insta::assert_snapshot!(
        "remote_pre_turn_compaction_strips_incoming_model_switch_shapes",
        format_labeled_requests_snapshot(
            "Remote pre-turn compaction during model switch currently excludes incoming user input, strips incoming <model_switch> from the compact request payload, and restores it in the post-compaction follow-up request.",
            &[
                ("Initial Request (Previous Model)", &initial_turn_request),
                ("Remote Compaction Request", &compact_request),
                (
                    "Remote Post-Compaction History Layout",
                    &post_compact_turn_request
                ),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// TODO(ccunningham): Update once remote pre-turn compaction context-overflow handling includes
// incoming user input and emits richer oversized-input messaging.
async fn snapshot_request_shape_remote_pre_turn_compaction_context_window_exceeded() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let harness = TestPraxisHarness::with_builder(
        test_praxis()
            .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
            .with_config(|config| {
                config.model_auto_compact_token_limit = Some(200);
            }),
    )
    .await?;
    let praxis = harness.test().thread.clone();

    let responses_mock = responses::mount_sse_sequence(
        harness.server(),
        vec![responses::sse(vec![
            responses::ev_assistant_message("m1", "REMOTE_FIRST_REPLY"),
            responses::ev_completed_with_tokens("r1", /*total_tokens*/ 500),
        ])],
    )
    .await;

    let compact_mock = responses::mount_compact_response_once(
        harness.server(),
        ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {
                "code": "context_length_exceeded",
                "message": "Your input exceeds the context window of this model. Please adjust your input and try again."
            }
        })),
    )
    .await;
    let post_compact_turn_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_assistant_message("m2", "REMOTE_POST_COMPACT_SHOULD_NOT_RUN"),
            responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
        ]),
    )
    .await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "USER_TWO".to_string(),
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
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(
        requests.len(),
        1,
        "expected no post-compaction follow-up turn request after compact failure"
    );
    assert!(
        post_compact_turn_mock.requests().is_empty(),
        "expected turn to stop after compaction failure"
    );

    let include_attempt_request = compact_mock.single_request();
    insta::assert_snapshot!(
        "remote_pre_turn_compaction_context_window_exceeded_shapes",
        format_labeled_requests_snapshot(
            "Remote pre-turn auto-compaction context-window failure: compaction request excludes the incoming user message and the turn errors.",
            &[(
                "Remote Compaction Request (Incoming User Excluded)",
                &include_attempt_request
            ),]
        )
    );
    assert!(
        error_message.to_lowercase().contains("context window"),
        "expected context window failure to surface, got {error_message}"
    );

    Ok(())
}

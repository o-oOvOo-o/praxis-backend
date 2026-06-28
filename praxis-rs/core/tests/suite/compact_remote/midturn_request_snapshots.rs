#![allow(clippy::expect_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_mid_turn_continuation_compaction() -> Result<()> {
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
                responses::ev_function_call("call-remote-mid-turn", DUMMY_FUNCTION_NAME, "{}"),
                responses::ev_completed_with_tokens("r1", /*total_tokens*/ 500),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m2", "REMOTE_MID_TURN_FINAL_REPLY"),
                responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
            ]),
        ],
    )
    .await;

    let compact_mock = responses::mount_compact_user_history_with_summary_once(
        harness.server(),
        &summary_with_prefix("REMOTE_MID_TURN_SUMMARY"),
    )
    .await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(
        requests.len(),
        2,
        "expected initial and post-compact requests"
    );

    let compact_request = compact_mock.single_request();
    insta::assert_snapshot!(
        "remote_mid_turn_compaction_shapes",
        format_labeled_requests_snapshot(
            "Remote mid-turn continuation compaction after tool output: compact request includes tool artifacts and the follow-up request includes the returned compaction item.",
            &[
                ("Remote Compaction Request", &compact_request),
                ("Remote Post-Compaction History Layout", &requests[1]),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_mid_turn_compaction_summary_only_reinjects_context()
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

    let initial_turn_request_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_function_call("call-remote-summary-only", DUMMY_FUNCTION_NAME, "{}"),
            responses::ev_completed_with_tokens("r1", /*total_tokens*/ 500),
        ]),
    )
    .await;
    let post_compact_turn_request_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_assistant_message("m2", "REMOTE_SUMMARY_ONLY_FINAL_REPLY"),
            responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
        ]),
    )
    .await;

    let compacted_history = vec![ResponseItem::Compaction {
        encrypted_content: summary_with_prefix("REMOTE_SUMMARY_ONLY"),
    }];
    let compact_mock = responses::mount_compact_json_once(
        harness.server(),
        serde_json::json!({ "output": compacted_history }),
    )
    .await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 1);
    assert_eq!(
        initial_turn_request_mock.requests().len(),
        1,
        "expected initial turn request"
    );
    assert_eq!(
        post_compact_turn_request_mock.requests().len(),
        1,
        "expected post-compaction request"
    );

    let compact_request = compact_mock.single_request();
    let post_compact_turn_request = post_compact_turn_request_mock.single_request();
    insta::assert_snapshot!(
        "remote_mid_turn_compaction_summary_only_reinjects_context_shapes",
        format_labeled_requests_snapshot(
            "Remote mid-turn compaction where compact output has only a compaction item: continuation layout reinjects context before that compaction item.",
            &[
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
async fn snapshot_request_shape_remote_mid_turn_compaction_multi_summary_reinjects_above_last_summary()
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

    let setup_turn_request_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_assistant_message("setup", "REMOTE_SETUP_REPLY"),
            responses::ev_completed_with_tokens("setup-response", /*total_tokens*/ 60),
        ]),
    )
    .await;
    let second_turn_request_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_shell_command_call("call-remote-multi-summary", "echo multi-summary"),
            responses::ev_completed_with_tokens("r1", /*total_tokens*/ 1_000),
        ]),
    )
    .await;

    let compact_mock = responses::mount_compact_user_history_with_summary_sequence(
        harness.server(),
        vec![
            summary_with_prefix("REMOTE_OLDER_SUMMARY"),
            summary_with_prefix("REMOTE_LATEST_SUMMARY"),
        ],
    )
    .await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    praxis.submit(Op::Compact).await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_TWO".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 2);
    assert_eq!(
        setup_turn_request_mock.requests().len(),
        1,
        "expected setup turn request"
    );
    assert_eq!(
        second_turn_request_mock.requests().len(),
        1,
        "expected second-turn pre-compaction request"
    );

    let compact_requests = compact_mock.requests();
    assert_eq!(
        compact_requests.len(),
        2,
        "expected one setup compact and one mid-turn compact request"
    );
    let compact_request = compact_requests[1].clone();
    let second_turn_request = second_turn_request_mock.single_request();
    assert!(
        compact_request.body_contains_text("REMOTE_OLDER_SUMMARY"),
        "older summary should round-trip from conversation history into the next compact request"
    );
    insta::assert_snapshot!(
        "remote_mid_turn_compaction_multi_summary_reinjects_above_last_summary_shapes",
        format_labeled_requests_snapshot(
            "After a prior manual /compact produced an older remote compaction item, the next turn hits remote auto-compaction before the next sampling request. The compact request carries forward that earlier compaction item, and the next sampling request shows the latest compaction item with context reinjected before USER_TWO.",
            &[
                ("Remote Compaction Request", &compact_request),
                (
                    "Second Turn Request (After Compaction)",
                    &second_turn_request
                ),
            ]
        )
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_manual_compact_without_previous_user_messages() -> Result<()>
{
    skip_if_no_network!(Ok(()));

    let harness = TestPraxisHarness::with_builder(
        test_praxis().with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing()),
    )
    .await?;
    let praxis = harness.test().thread.clone();

    let responses_mock = responses::mount_sse_once(
        harness.server(),
        responses::sse(vec![
            responses::ev_assistant_message("m1", "REMOTE_MANUAL_EMPTY_FOLLOW_UP_REPLY"),
            responses::ev_completed_with_tokens("r1", /*total_tokens*/ 80),
        ]),
    )
    .await;

    let compact_mock =
        responses::mount_compact_json_once(harness.server(), serde_json::json!({ "output": [] }))
            .await;

    praxis.submit(Op::Compact).await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(
        compact_mock.requests().len(),
        0,
        "manual /compact without prior user should not issue a remote compaction request"
    );
    let follow_up_request = responses_mock.single_request();
    insta::assert_snapshot!(
        "remote_manual_compact_without_prev_user_shapes",
        format_labeled_requests_snapshot(
            "Remote manual /compact with no prior user turn skips the remote compact request; the follow-up turn carries canonical context and new user message.",
            &[("Remote Post-Compaction History Layout", &follow_up_request)]
        )
    );

    Ok(())
}

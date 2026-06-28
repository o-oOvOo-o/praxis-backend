#![allow(clippy::expect_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_pre_turn_compaction_restates_realtime_start() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = wiremock::MockServer::start().await;
    let realtime_server = start_remote_realtime_server().await;
    let mut builder = remote_realtime_test_praxis_builder(&realtime_server).with_config(|config| {
        config.model_auto_compact_token_limit = Some(200);
    });
    let test = builder.build(&server).await?;

    let responses_mock = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_assistant_message("m1", "REMOTE_FIRST_REPLY"),
                responses::ev_completed_with_tokens("r1", /*total_tokens*/ 500),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m2", "REMOTE_SECOND_REPLY"),
                responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
            ]),
        ],
    )
    .await;
    let compact_mock = responses::mount_compact_json_once(
        &server,
        serde_json::json!({
            "output": compacted_summary_only_output(
                "REMOTE_PRETURN_REALTIME_STILL_ACTIVE_SUMMARY"
            )
        }),
    )
    .await;

    start_realtime_conversation(test.thread.as_ref()).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_TWO".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let compact_request = compact_mock.single_request();
    let post_compact_request = &requests[1];
    assert_request_contains_realtime_start(post_compact_request);

    insta::assert_snapshot!(
        "remote_pre_turn_compaction_restates_realtime_start_shapes",
        format_labeled_requests_snapshot(
            "Remote pre-turn auto-compaction while realtime remains active: compaction clears the reference baseline, so the follow-up request restates realtime-start instructions.",
            &[
                ("Remote Compaction Request", &compact_request),
                (
                    "Remote Post-Compaction History Layout",
                    post_compact_request
                ),
            ]
        )
    );

    close_realtime_conversation(test.thread.as_ref()).await?;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_request_uses_custom_experimental_realtime_start_instructions() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = wiremock::MockServer::start().await;
    let realtime_server = start_remote_realtime_server().await;
    let custom_instructions = "custom realtime start instructions";
    let mut builder = remote_realtime_test_praxis_builder(&realtime_server).with_config({
        let custom_instructions = custom_instructions.to_string();
        move |config| {
            config.experimental_realtime_start_instructions = Some(custom_instructions);
        }
    });
    let test = builder.build(&server).await?;

    let responses_mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_assistant_message("m1", "REMOTE_FIRST_REPLY"),
            responses::ev_completed("r1"),
        ]),
    )
    .await;

    start_realtime_conversation(test.thread.as_ref()).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_request_contains_custom_realtime_start(
        &responses_mock.single_request(),
        custom_instructions,
    );

    close_realtime_conversation(test.thread.as_ref()).await?;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_pre_turn_compaction_restates_realtime_end() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = wiremock::MockServer::start().await;
    let realtime_server = start_remote_realtime_server().await;
    let mut builder = remote_realtime_test_praxis_builder(&realtime_server).with_config(|config| {
        config.model_auto_compact_token_limit = Some(200);
    });
    let test = builder.build(&server).await?;

    let responses_mock = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_assistant_message("m1", "REMOTE_FIRST_REPLY"),
                responses::ev_completed_with_tokens("r1", /*total_tokens*/ 500),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m2", "REMOTE_SECOND_REPLY"),
                responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
            ]),
        ],
    )
    .await;
    let compact_mock = responses::mount_compact_json_once(
        &server,
        serde_json::json!({
            "output": compacted_summary_only_output(
                "REMOTE_PRETURN_REALTIME_CLOSED_SUMMARY"
            )
        }),
    )
    .await;

    start_realtime_conversation(test.thread.as_ref()).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    close_realtime_conversation(test.thread.as_ref()).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_TWO".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let compact_request = compact_mock.single_request();
    let post_compact_request = &requests[1];
    assert_request_contains_realtime_end(post_compact_request);

    insta::assert_snapshot!(
        "remote_pre_turn_compaction_restates_realtime_end_shapes",
        format_labeled_requests_snapshot(
            "Remote pre-turn auto-compaction after realtime was closed between turns: the follow-up request emits realtime-end instructions from previous-turn settings even though compaction cleared the reference baseline.",
            &[
                ("Remote Compaction Request", &compact_request),
                (
                    "Remote Post-Compaction History Layout",
                    post_compact_request
                ),
            ]
        )
    );

    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_manual_compact_restates_realtime_start() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = wiremock::MockServer::start().await;
    let realtime_server = start_remote_realtime_server().await;
    let mut builder = remote_realtime_test_praxis_builder(&realtime_server);
    let test = builder.build(&server).await?;

    let responses_mock = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_assistant_message("m1", "REMOTE_FIRST_REPLY"),
                responses::ev_completed_with_tokens("r1", /*total_tokens*/ 60),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m2", "REMOTE_SECOND_REPLY"),
                responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
            ]),
        ],
    )
    .await;
    let compact_mock = responses::mount_compact_json_once(
        &server,
        serde_json::json!({
            "output": compacted_summary_only_output(
                "REMOTE_MANUAL_REALTIME_STILL_ACTIVE_SUMMARY"
            )
        }),
    )
    .await;

    start_realtime_conversation(test.thread.as_ref()).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread.submit(Op::Compact).await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_TWO".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let compact_request = compact_mock.single_request();
    let post_compact_request = &requests[1];
    assert_request_contains_realtime_start(post_compact_request);

    insta::assert_snapshot!(
        "remote_manual_compact_restates_realtime_start_shapes",
        format_labeled_requests_snapshot(
            "Remote manual /compact while realtime remains active: the next regular turn restates realtime-start instructions after compaction clears the baseline.",
            &[
                ("Remote Compaction Request", &compact_request),
                (
                    "Remote Post-Compaction History Layout",
                    post_compact_request
                ),
            ]
        )
    );

    close_realtime_conversation(test.thread.as_ref()).await?;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_mid_turn_compaction_does_not_restate_realtime_end()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = wiremock::MockServer::start().await;
    let realtime_server = start_remote_realtime_server().await;
    let mut builder = remote_realtime_test_praxis_builder(&realtime_server).with_config(|config| {
        config.model_auto_compact_token_limit = Some(200);
    });
    let test = builder.build(&server).await?;

    let responses_mock = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_assistant_message("setup", "REMOTE_SETUP_REPLY"),
                responses::ev_completed_with_tokens("setup-response", /*total_tokens*/ 60),
            ]),
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
    let compact_mock = responses::mount_compact_json_once(
        &server,
        serde_json::json!({
            "output": compacted_summary_only_output(
                "REMOTE_MID_TURN_REALTIME_CLOSED_SUMMARY"
            )
        }),
    )
    .await;

    start_realtime_conversation(test.thread.as_ref()).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "SETUP_USER".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    close_realtime_conversation(test.thread.as_ref()).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_TWO".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(requests.len(), 3, "expected three model requests");

    let second_turn_request = &requests[1];
    let compact_request = compact_mock.single_request();
    let post_compact_request = &requests[2];
    assert_request_contains_realtime_end(second_turn_request);
    assert!(
        !post_compact_request
            .body_json()
            .to_string()
            .contains("<realtime_conversation>"),
        "did not expect post-compaction history to restate realtime instructions once the current turn had already established an inactive baseline"
    );

    insta::assert_snapshot!(
        "remote_mid_turn_compaction_does_not_restate_realtime_end_shapes",
        format_labeled_requests_snapshot(
            "Remote mid-turn continuation compaction after realtime was closed before the turn: the initial second-turn request emits realtime-end instructions, but the continuation request does not restate them after compaction because the current turn already established the inactive baseline.",
            &[
                ("Second Turn Initial Request", second_turn_request),
                ("Remote Compaction Request", &compact_request),
                (
                    "Remote Post-Compaction History Layout",
                    post_compact_request
                ),
            ]
        )
    );

    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_request_shape_remote_compact_resume_restates_realtime_end() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = wiremock::MockServer::start().await;
    let realtime_server = start_remote_realtime_server().await;
    let mut builder = remote_realtime_test_praxis_builder(&realtime_server);
    let initial = builder.build(&server).await?;
    let home = initial.home.clone();
    let rollout_path = initial
        .session_configured
        .rollout_path
        .clone()
        .expect("rollout path");

    let responses_mock = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_assistant_message("m1", "REMOTE_FIRST_REPLY"),
                responses::ev_completed_with_tokens("r1", /*total_tokens*/ 60),
            ]),
            responses::sse(vec![
                responses::ev_assistant_message("m2", "REMOTE_AFTER_RESUME_REPLY"),
                responses::ev_completed_with_tokens("r2", /*total_tokens*/ 80),
            ]),
        ],
    )
    .await;
    let compact_mock = responses::mount_compact_json_once(
        &server,
        serde_json::json!({
            "output": compacted_summary_only_output(
                "REMOTE_RESUME_REALTIME_CLOSED_SUMMARY"
            )
        }),
    )
    .await;

    start_realtime_conversation(initial.thread.as_ref()).await?;

    initial
        .thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_ONE".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&initial.thread, |ev| {
        matches!(ev, EventMsg::TurnComplete(_))
    })
    .await;

    close_realtime_conversation(initial.thread.as_ref()).await?;

    initial.thread.submit(Op::Compact).await?;
    wait_for_event(&initial.thread, |ev| {
        matches!(ev, EventMsg::TurnComplete(_))
    })
    .await;

    initial.thread.submit(Op::Shutdown).await?;
    wait_for_event(&initial.thread, |ev| {
        matches!(ev, EventMsg::ShutdownComplete)
    })
    .await;

    let mut resume_builder =
        test_praxis().with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing());
    let resumed = resume_builder.resume(&server, home, rollout_path).await?;

    resumed
        .thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "USER_TWO".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;
    wait_for_event(&resumed.thread, |ev| {
        matches!(ev, EventMsg::TurnComplete(_))
    })
    .await;

    assert_eq!(compact_mock.requests().len(), 1);
    let requests = responses_mock.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let compact_request = compact_mock.single_request();
    let after_resume_request = &requests[1];
    assert_request_contains_realtime_end(after_resume_request);

    insta::assert_snapshot!(
        "remote_compact_resume_restates_realtime_end_shapes",
        format_labeled_requests_snapshot(
            "After remote manual /compact and resume, the first resumed turn rebuilds history from the compaction item and restates realtime-end instructions from reconstructed previous-turn settings.",
            &[
                ("Remote Compaction Request", &compact_request),
                ("Remote Post-Resume History Layout", after_resume_request),
            ]
        )
    );

    realtime_server.shutdown().await;
    Ok(())
}

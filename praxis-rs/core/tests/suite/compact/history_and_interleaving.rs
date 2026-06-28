#![allow(clippy::expect_used)]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_compact_twice_preserves_latest_user_messages() {
    skip_if_no_network!();

    let first_user_message = "first manual turn";
    let second_user_message = "second manual turn";
    let final_user_message = "post compact follow-up";
    let first_summary = "FIRST_MANUAL_SUMMARY";
    let second_summary = "SECOND_MANUAL_SUMMARY";
    let expected_second_summary = summary_with_prefix(second_summary);

    let server = start_mock_server().await;

    let first_turn = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed("r1"),
    ]);
    let first_compact_summary = auto_summary(first_summary);
    let first_compact = sse(vec![
        ev_assistant_message("m2", &first_compact_summary),
        ev_completed("r2"),
    ]);
    let second_turn = sse(vec![
        ev_assistant_message("m3", SECOND_LARGE_REPLY),
        ev_completed("r3"),
    ]);
    let second_compact_summary = auto_summary(second_summary);
    let second_compact = sse(vec![
        ev_assistant_message("m4", &second_compact_summary),
        ev_completed("r4"),
    ]);
    let final_turn = sse(vec![
        ev_assistant_message("m5", FINAL_REPLY),
        ev_completed("r5"),
    ]);

    let responses_mock = mount_sse_sequence(
        &server,
        vec![
            first_turn,
            first_compact,
            second_turn,
            second_compact,
            final_turn,
        ],
    )
    .await;

    let model_provider = non_openai_model_provider(&server);

    let mut builder = test_praxis().with_config(move |config| {
        config.model_provider = model_provider;
        set_test_compact_prompt(config);
    });
    let praxis = builder.build(&server).await.unwrap().thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: first_user_message.into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    praxis.submit(Op::Compact).await.unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: second_user_message.into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    praxis.submit(Op::Compact).await.unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: final_user_message.into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = responses_mock.requests();
    assert_eq!(
        requests.len(),
        5,
        "expected exactly 5 requests (user turn, compact, user turn, compact, final turn)"
    );
    let contains_user_text = |request: &core_test_support::responses::ResponsesRequest,
                              expected: &str| {
        request
            .message_input_texts("user")
            .iter()
            .any(|text| text == expected)
    };

    assert!(
        contains_user_text(&requests[0], first_user_message),
        "first turn request missing first user message"
    );
    assert!(
        !contains_user_text(&requests[0], SUMMARIZATION_PROMPT),
        "first turn request should not include summarization prompt"
    );

    assert!(
        contains_user_text(&requests[1], first_user_message),
        "first compact request should include history before compaction"
    );

    assert!(
        contains_user_text(&requests[2], second_user_message),
        "second turn request missing second user message"
    );
    assert!(
        contains_user_text(&requests[2], first_user_message),
        "second turn request should include the compacted user history"
    );

    assert!(
        contains_user_text(&requests[3], second_user_message),
        "second compact request should include latest history"
    );

    insta::assert_snapshot!(
        "manual_compact_with_history_shapes",
        format_labeled_requests_snapshot(
            "Manual /compact with prior user history compacts existing history and the follow-up turn includes the compact summary plus new user message.",
            &[
                ("Local Compaction Request", &requests[1]),
                ("Local Post-Compaction History Layout", &requests[2]),
            ]
        )
    );

    let first_compact_has_prompt = contains_user_text(&requests[1], SUMMARIZATION_PROMPT);
    let second_compact_has_prompt = contains_user_text(&requests[3], SUMMARIZATION_PROMPT);
    assert_eq!(
        first_compact_has_prompt, second_compact_has_prompt,
        "compact requests should consistently include or omit the summarization prompt"
    );

    let first_request_user_texts = requests[0].message_input_texts("user");
    let first_turn_user_index = first_request_user_texts
        .len()
        .checked_sub(1)
        .unwrap_or_else(|| panic!("first turn request missing user messages"));
    assert_eq!(
        first_request_user_texts[first_turn_user_index], first_user_message,
        "first turn request should end with the submitted user message"
    );
    let initial_seeded_user_prefix = &first_request_user_texts[..first_turn_user_index];

    let final_request_user_texts = requests
        .last()
        .unwrap_or_else(|| panic!("final turn request missing for {final_user_message}"))
        .message_input_texts("user");
    assert!(
        !initial_seeded_user_prefix.is_empty(),
        "first turn should include seeded user prefix before the submitted user message"
    );
    let (final_request_last_user_text, final_request_before_last_user) = final_request_user_texts
        .split_last()
        .unwrap_or_else(|| panic!("final turn request missing user messages"));
    assert_eq!(
        final_request_last_user_text, final_user_message,
        "final turn request should end with the submitted user message"
    );
    let history_before_seeded_prefix = final_request_before_last_user
        .strip_suffix(initial_seeded_user_prefix)
        .unwrap_or_else(|| {
            panic!(
                "final request should end with the seeded user prefix from the first request: {initial_seeded_user_prefix:?}"
            )
        });
    let expected_history = vec![
        first_user_message.to_string(),
        second_user_message.to_string(),
        expected_second_summary,
    ];
    assert_eq!(history_before_seeded_prefix, expected_history.as_slice());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_compact_allows_multiple_attempts_when_interleaved_with_other_turn_events() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let sse1 = sse(vec![
        ev_assistant_message("m1", FIRST_REPLY),
        ev_completed_with_tokens("r1", /*total_tokens*/ 500),
    ]);
    let first_summary_payload = auto_summary(FIRST_AUTO_SUMMARY);
    let sse2 = sse(vec![
        ev_assistant_message("m2", &first_summary_payload),
        ev_completed_with_tokens("r2", /*total_tokens*/ 50),
    ]);
    let sse3 = sse(vec![
        ev_function_call(DUMMY_CALL_ID, DUMMY_FUNCTION_NAME, "{}"),
        ev_completed_with_tokens("r3", /*total_tokens*/ 150),
    ]);
    let sse4 = sse(vec![
        ev_assistant_message("m4", SECOND_LARGE_REPLY),
        ev_completed_with_tokens("r4", /*total_tokens*/ 450),
    ]);
    let second_summary_payload = auto_summary(SECOND_AUTO_SUMMARY);
    let sse5 = sse(vec![
        ev_assistant_message("m5", &second_summary_payload),
        ev_completed_with_tokens("r5", /*total_tokens*/ 60),
    ]);
    let sse6 = sse(vec![
        ev_assistant_message("m6", FINAL_REPLY),
        ev_completed_with_tokens("r6", /*total_tokens*/ 120),
    ]);
    let follow_up_user = "FOLLOW_UP_AUTO_COMPACT";
    let final_user = "FINAL_AUTO_COMPACT";

    let request_log = mount_sse_sequence(&server, vec![sse1, sse2, sse3, sse4, sse5, sse6]).await;

    let model_provider = non_openai_model_provider(&server);

    let mut builder = test_praxis().with_config(move |config| {
        config.model_provider = model_provider;
        set_test_compact_prompt(config);
        config.model_auto_compact_token_limit = Some(200);
    });
    let praxis = builder.build(&server).await.unwrap().thread;

    let mut auto_compact_lifecycle_events = Vec::new();
    for user in [MULTI_AUTO_MSG, follow_up_user, final_user] {
        codex
            .submit_user_turn(
                vec![UserInput::Text {
                    text: user.into(),
                    text_elements: Vec::new(),
                }],
                None,
            )
            .await
            .unwrap();

        loop {
            let event = praxis.next_event().await.unwrap();
            if event.id.starts_with("auto-compact-")
                && matches!(
                    event.msg,
                    EventMsg::TurnStarted(_) | EventMsg::TurnComplete(_)
                )
            {
                auto_compact_lifecycle_events.push(event);
                continue;
            }
            if let EventMsg::TurnComplete(_) = &event.msg
                && !event.id.starts_with("auto-compact-")
            {
                break;
            }
        }
    }

    assert!(
        auto_compact_lifecycle_events.is_empty(),
        "auto compact should not emit task lifecycle events"
    );

    let request_bodies: Vec<String> = request_log
        .requests()
        .into_iter()
        .map(|request| request.body_json().to_string())
        .collect();
    assert_eq!(
        request_bodies.len(),
        6,
        "expected six requests including two auto compactions"
    );
    assert!(
        request_bodies[0].contains(MULTI_AUTO_MSG),
        "first request should contain the user input"
    );
    assert!(
        body_contains_text(&request_bodies[1], SUMMARIZATION_PROMPT),
        "first auto compact request should include the summarization prompt"
    );
    assert!(
        request_bodies[3].contains(&format!("unsupported call: {DUMMY_FUNCTION_NAME}")),
        "function call output should be sent before the second auto compact"
    );
    assert!(
        body_contains_text(&request_bodies[4], SUMMARIZATION_PROMPT),
        "second auto compact request should include the summarization prompt"
    );
}

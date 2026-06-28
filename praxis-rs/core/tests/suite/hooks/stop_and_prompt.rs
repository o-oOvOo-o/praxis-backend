use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_hook_can_block_multiple_times_in_same_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_assistant_message("msg-1", "draft one"),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-2", "draft two"),
                ev_completed("resp-2"),
            ]),
            sse(vec![
                ev_response_created("resp-3"),
                ev_assistant_message("msg-3", "final draft"),
                ev_completed("resp-3"),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) = write_stop_hook(
                home,
                &[FIRST_CONTINUATION_PROMPT, SECOND_CONTINUATION_PROMPT],
            ) {
                panic!("failed to write stop hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("hello from the sea").await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        request_hook_prompt_texts(&requests[1]),
        vec![FIRST_CONTINUATION_PROMPT.to_string()],
        "second request should include the first continuation prompt as user hook context",
    );
    assert_eq!(
        request_hook_prompt_texts(&requests[2]),
        vec![
            FIRST_CONTINUATION_PROMPT.to_string(),
            SECOND_CONTINUATION_PROMPT.to_string(),
        ],
        "third request should retain hook prompts in user history",
    );

    let hook_inputs = read_stop_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 3);
    let stop_turn_ids = hook_inputs
        .iter()
        .map(|input| {
            input["turn_id"]
                .as_str()
                .expect("stop hook input turn_id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert!(
        stop_turn_ids.iter().all(|turn_id| !turn_id.is_empty()),
        "stop hook turn ids should be non-empty",
    );
    let first_stop_turn_id = stop_turn_ids
        .first()
        .expect("stop hook inputs should include a first turn id")
        .clone();
    assert_eq!(
        stop_turn_ids,
        vec![
            first_stop_turn_id.clone(),
            first_stop_turn_id.clone(),
            first_stop_turn_id,
        ],
    );
    assert_eq!(
        hook_inputs
            .iter()
            .map(|input| input["stop_hook_active"]
                .as_bool()
                .expect("stop_hook_active bool"))
            .collect::<Vec<_>>(),
        vec![false, true, true],
    );

    let rollout_path = test.thread.rollout_path().expect("rollout path");
    let rollout_text = fs::read_to_string(&rollout_path)?;
    let hook_prompt_texts = rollout_hook_prompt_texts(&rollout_text)?;
    assert!(
        hook_prompt_texts.contains(&FIRST_CONTINUATION_PROMPT.to_string()),
        "rollout should persist the first continuation prompt",
    );
    assert!(
        hook_prompt_texts.contains(&SECOND_CONTINUATION_PROMPT.to_string()),
        "rollout should persist the second continuation prompt",
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_start_hook_sees_materialized_transcript_path() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let _response = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "hello from the reef"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) = write_session_start_hook_recording_transcript(home) {
                panic!("failed to write session start hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("hello").await?;

    let hook_inputs = read_session_start_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(
        hook_inputs[0]
            .get("transcript_path")
            .and_then(Value::as_str)
            .map(str::is_empty),
        Some(false)
    );
    assert_eq!(hook_inputs[0].get("exists"), Some(&Value::Bool(true)));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resumed_thread_keeps_stop_continuation_prompt_in_history() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let initial_responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_assistant_message("msg-1", "initial draft"),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-2", "revised draft"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut initial_builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) = write_stop_hook(home, &[FIRST_CONTINUATION_PROMPT]) {
                panic!("failed to write stop hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let initial = initial_builder.build(&server).await?;
    let home = initial.home.clone();
    let rollout_path = initial
        .session_configured
        .rollout_path
        .clone()
        .expect("rollout path");

    initial.submit_turn("tell me something").await?;

    assert_eq!(initial_responses.requests().len(), 2);

    let resumed_response = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            ev_assistant_message("msg-3", "fresh turn after resume"),
            ev_completed("resp-3"),
        ]),
    )
    .await;

    let mut resume_builder = test_praxis().with_config(|config| {
        config
            .features
            .enable(Feature::PraxisHooks)
            .expect("test config should allow feature update");
    });
    let resumed = resume_builder.resume(&server, home, rollout_path).await?;

    resumed.submit_turn("and now continue").await?;

    let resumed_request = resumed_response.single_request();
    assert_eq!(
        request_hook_prompt_texts(&resumed_request),
        vec![FIRST_CONTINUATION_PROMPT.to_string()],
        "resumed request should keep the persisted continuation prompt in user history",
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiple_blocking_stop_hooks_persist_multiple_hook_prompt_fragments() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_assistant_message("msg-1", "draft one"),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-2", "final draft"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) = write_parallel_stop_hooks(
                home,
                &[FIRST_CONTINUATION_PROMPT, SECOND_CONTINUATION_PROMPT],
            ) {
                panic!("failed to write parallel stop hook fixtures: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("hello again").await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        request_hook_prompt_texts(&requests[1]),
        vec![
            FIRST_CONTINUATION_PROMPT.to_string(),
            SECOND_CONTINUATION_PROMPT.to_string(),
        ],
        "second request should receive one user hook prompt message with both fragments",
    );

    let rollout_path = test.thread.rollout_path().expect("rollout path");
    let rollout_text = fs::read_to_string(&rollout_path)?;
    assert_eq!(
        rollout_hook_prompt_texts(&rollout_text)?,
        vec![
            FIRST_CONTINUATION_PROMPT.to_string(),
            SECOND_CONTINUATION_PROMPT.to_string(),
        ],
        "rollout should preserve both hook prompt fragments in order",
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blocked_user_prompt_submit_persists_additional_context_for_next_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "second prompt handled"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_user_prompt_submit_hook(home, "blocked first prompt", BLOCKED_PROMPT_CONTEXT)
            {
                panic!("failed to write user prompt submit hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("blocked first prompt").await?;
    test.submit_turn("second prompt").await?;

    let request = response.single_request();
    assert!(
        request
            .message_input_texts("developer")
            .contains(&BLOCKED_PROMPT_CONTEXT.to_string()),
        "second request should include developer context persisted from the blocked prompt",
    );
    assert!(
        request
            .message_input_texts("user")
            .iter()
            .all(|text| !text.contains("blocked first prompt")),
        "blocked prompt should not be sent to the model",
    );
    assert!(
        request
            .message_input_texts("user")
            .iter()
            .any(|text| text.contains("second prompt")),
        "second request should include the accepted prompt",
    );

    let hook_inputs = read_user_prompt_submit_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 2);
    assert_eq!(
        hook_inputs
            .iter()
            .map(|input| {
                input["prompt"]
                    .as_str()
                    .expect("user prompt submit hook prompt")
                    .to_string()
            })
            .collect::<Vec<_>>(),
        vec![
            "blocked first prompt".to_string(),
            "second prompt".to_string()
        ],
    );
    assert!(
        hook_inputs.iter().all(|input| input["turn_id"]
            .as_str()
            .is_some_and(|turn_id| !turn_id.is_empty())),
        "blocked and accepted prompt hooks should both receive a non-empty turn_id",
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blocked_queued_prompt_does_not_strand_earlier_accepted_prompt() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let (gate_completed_tx, gate_completed_rx) = oneshot::channel();
    let first_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_response_created("resp-1")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_message_item_added("msg-1", "")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_output_text_delta("first ")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(ev_message_item_done("msg-1", "first response")),
        },
        StreamingSseChunk {
            gate: Some(gate_completed_rx),
            body: sse_event(ev_completed("resp-1")),
        },
    ];
    let second_chunks = vec![StreamingSseChunk {
        gate: None,
        body: sse(vec![
            ev_response_created("resp-2"),
            ev_assistant_message("msg-2", "accepted queued prompt handled"),
            ev_completed("resp-2"),
        ]),
    }];
    let (server, _completions) =
        start_streaming_sse_server(vec![first_chunks, second_chunks]).await;

    let mut builder = test_praxis()
        .with_model("gpt-5.1")
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_user_prompt_submit_hook(home, "blocked queued prompt", BLOCKED_PROMPT_CONTEXT)
            {
                panic!("failed to write user prompt submit hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build_with_streaming_server(&server).await?;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "initial prompt".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;

    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::AgentMessageContentDelta(_))
    })
    .await;

    for text in ["accepted queued prompt", "blocked queued prompt"] {
        test.thread
            .submit_user_turn(
                vec![UserInput::Text {
                    text: text.to_string(),
                    text_elements: Vec::new(),
                }],
                None,
            )
            .await?;
    }

    sleep(Duration::from_millis(100)).await;
    let _ = gate_completed_tx.send(());

    let requests = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let requests = server.requests().await;
            if requests.len() >= 2 {
                break requests;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("second request should arrive")
    .into_iter()
    .collect::<Vec<_>>();

    sleep(Duration::from_millis(100)).await;

    assert_eq!(requests.len(), 2);

    let second_user_texts = request_message_input_texts(&requests[1], "user");
    assert!(
        second_user_texts.contains(&"accepted queued prompt".to_string()),
        "second request should include the accepted queued prompt",
    );
    assert!(
        !second_user_texts.contains(&"blocked queued prompt".to_string()),
        "second request should not include the blocked queued prompt",
    );

    let hook_inputs = read_user_prompt_submit_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 3);
    assert_eq!(
        hook_inputs
            .iter()
            .map(|input| {
                input["prompt"]
                    .as_str()
                    .expect("queued prompt hook prompt")
                    .to_string()
            })
            .collect::<Vec<_>>(),
        vec![
            "initial prompt".to_string(),
            "accepted queued prompt".to_string(),
            "blocked queued prompt".to_string(),
        ],
    );
    let queued_turn_ids = hook_inputs
        .iter()
        .map(|input| {
            input["turn_id"]
                .as_str()
                .expect("queued prompt hook turn_id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert!(
        queued_turn_ids.iter().all(|turn_id| !turn_id.is_empty()),
        "queued prompt hook turn ids should be non-empty",
    );
    let first_queued_turn_id = queued_turn_ids
        .first()
        .expect("queued prompt hook inputs should include a first turn id")
        .clone();
    assert_eq!(
        queued_turn_ids,
        vec![
            first_queued_turn_id.clone(),
            first_queued_turn_id.clone(),
            first_queued_turn_id,
        ],
    );

    server.shutdown().await;
    Ok(())
}

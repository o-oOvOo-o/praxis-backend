use super::*;

#[tokio::test]
async fn handle_responses_span_records_response_kind_and_tool_name() {
    let buffer: &'static Mutex<Vec<u8>> = Box::leak(Box::new(Mutex::new(Vec::new())));
    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .with_ansi(false)
        .with_max_level(Level::TRACE)
        .with_span_events(FmtSpan::FULL)
        .with_writer(MockWriter::new(buffer))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_function_call("function-call", "nonexistent", "{\"value\":1}"),
            ev_completed("done"),
        ]),
    )
    .await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "tool handled"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

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

    let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();

    assert!(
        logs.contains("handle_responses{otel.name=\"function_call\"")
            && logs.contains("tool_name=\"nonexistent\"")
            && logs.contains("from=\"output_item_done\""),
        "missing handle_responses span with function call metadata\nlogs:\n{logs}"
    );
    assert!(
        logs.contains("handle_responses{otel.name=\"completed\""),
        "missing handle_responses span for completion\nlogs:\n{logs}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn record_responses_sets_span_fields_for_response_events() {
    let buffer: &'static Mutex<Vec<u8>> = Box::leak(Box::new(Mutex::new(Vec::new())));
    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .with_ansi(false)
        .with_max_level(Level::TRACE)
        .with_span_events(FmtSpan::FULL)
        .with_writer(MockWriter::new(buffer))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let server = start_mock_server().await;

    let sse_body = sse(vec![
        ev_response_created("resp-1"),
        serde_json::json!({
            "type": "response.output_item.added",
            "item": {
                "type": "function_call",
                "call_id": "call-1",
                "name": "fn",
                "arguments": "{\"value\":1}"
            }
        }),
        ev_message_item_added("msg-added", "hi there"),
        ev_reasoning_item_added("reasoning-1", &["summary"]),
        ev_output_text_delta("delta"),
        ev_reasoning_summary_text_delta("summary-delta"),
        ev_reasoning_text_delta("raw-delta"),
        ev_function_call("call-1", "fn", "{\"key\":\"value\"}"),
        ev_assistant_message("msg-1", "agent"),
        ev_reasoning_item("reasoning-1", &["summary"], &[]),
        ev_completed("resp-1"),
    ]);

    mount_response_once(&server, sse_response(sse_body)).await;
    mount_response_once(
        &server,
        sse_response(sse(vec![
            ev_assistant_message("msg-2", "follow-up complete"),
            ev_completed("resp-2"),
        ])),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

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

    let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();

    let expected = [
        ("created", None::<&str>, None::<&str>),
        ("rate_limits", None, None),
        ("function_call", Some("output_item_added"), Some("fn")),
        ("message_from_assistant", Some("output_item_done"), None),
        ("reasoning", Some("output_item_done"), None),
        ("text_delta", None, None),
        ("reasoning_summary_delta", None, None),
        ("reasoning_content_delta", None, None),
        ("completed", None, None),
    ];

    for (name, from, tool_name) in expected {
        assert!(
            logs.contains(&format!("handle_responses{{otel.name=\"{name}\"")),
            "missing otel.name={name}\nlogs:\n{logs}"
        );
        if let Some(from) = from {
            assert!(
                logs.contains(&format!("from=\"{from}\"")),
                "missing from={from} for {name}\nlogs:\n{logs}"
            );
        }
        if let Some(tool_name) = tool_name {
            assert!(
                logs.contains(&format!("tool_name=\"{tool_name}\"")),
                "missing tool_name={tool_name} for {name}\nlogs:\n{logs}"
            );
        }
    }
}

#[tokio::test]
#[traced_test]
async fn handle_response_item_records_tool_result_for_custom_tool_call() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_custom_tool_call(
                "custom-tool-call",
                "unsupported_tool",
                "{\"key\":\"value\"}",
            ),
            ev_completed("done"),
        ]),
    )
    .await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

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

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(|lines: &[&str]| {
        let line = lines
            .iter()
            .find(|line| {
                line.contains("praxis.tool_result") && line.contains("call_id=custom-tool-call")
            })
            .ok_or_else(|| "missing praxis.tool_result event".to_string())?;

        if !line.contains("tool_name=unsupported_tool") {
            return Err("missing tool_name field".to_string());
        }
        if !line.contains("arguments={\"key\":\"value\"}") {
            return Err("missing arguments field".to_string());
        }
        if !line.contains("output=unsupported custom tool call: unsupported_tool") {
            return Err("missing output field".to_string());
        }
        if !line.contains("success=false") {
            return Err("missing success field".to_string());
        }
        assert_empty_mcp_tool_fields(line)?;

        Ok(())
    });
}

#[tokio::test]
#[traced_test]
async fn handle_response_item_records_tool_result_for_function_call() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_function_call("function-call", "nonexistent", "{\"value\":1}"),
            ev_completed("done"),
        ]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

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

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(|lines: &[&str]| {
        let line = lines
            .iter()
            .find(|line| {
                line.contains("praxis.tool_result") && line.contains("call_id=function-call")
            })
            .ok_or_else(|| "missing praxis.tool_result event".to_string())?;

        if !line.contains("tool_name=nonexistent") {
            return Err("missing tool_name field".to_string());
        }
        if !line.contains("arguments={\"value\":1}") {
            return Err("missing arguments field".to_string());
        }
        if !line.contains("output=unsupported call: nonexistent") {
            return Err("missing output field".to_string());
        }
        if !line.contains("success=false") {
            return Err("missing success field".to_string());
        }
        assert_empty_mcp_tool_fields(line)?;

        Ok(())
    });
}

#[tokio::test]
#[traced_test]
async fn handle_response_item_records_tool_result_for_local_shell_missing_ids() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            serde_json::json!({
                "type": "response.output_item.done",
                "item": {
                    "type": "local_shell_call",
                    "status": "completed",
                    "action": {
                        "type": "exec",
                        "command": vec!["/bin/echo", "hello"],
                    }
                }
            }),
            ev_completed("done"),
        ]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await
        .unwrap();

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

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(|lines: &[&str]| {
        let line = lines
            .iter()
            .find(|line| {
                line.contains("praxis.tool_result")
                    && line.contains(&"tool_name=local_shell".to_string())
                    && line.contains("output=LocalShellCall without call_id or id")
            })
            .ok_or_else(|| "missing praxis.tool_result event".to_string())?;

        if !line.contains("success=false") {
            return Err("missing success field".to_string());
        }
        assert_empty_mcp_tool_fields(line)?;

        Ok(())
    });
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[traced_test]
async fn handle_response_item_records_tool_result_for_local_shell_call() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call("shell-call", "completed", vec!["/bin/echo", "shell"]),
            ev_completed("done"),
        ]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::GhostCommit)
                .expect("test config should allow feature update");
            config.permissions.approval_policy = Constrained::allow_any(AskForApproval::Never);
        })
        .build(&server)
        .await
        .unwrap();

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

    logs_assert(|lines: &[&str]| {
        let line = lines
            .iter()
            .find(|line| line.contains("praxis.tool_result") && line.contains("call_id=shell-call"))
            .ok_or_else(|| "missing praxis.tool_result event".to_string())?;

        if !line.contains("tool_name=local_shell") {
            return Err("missing tool_name field".to_string());
        }
        if !line.contains("arguments=/bin/echo shell") {
            return Err("missing arguments field".to_string());
        }
        let output_idx = line
            .find("output=")
            .ok_or_else(|| "missing output field".to_string())?;
        if line[output_idx + "output=".len()..].is_empty() {
            return Err("empty output field".to_string());
        }
        if !line.contains("success=false") {
            return Err("missing success field".to_string());
        }
        assert_empty_mcp_tool_fields(line)?;

        Ok(())
    });
}

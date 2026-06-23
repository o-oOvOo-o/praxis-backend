#![cfg(not(target_os = "windows"))]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn js_repl_emit_image_attaches_local_image() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_praxis().with_config(|config| {
        config
            .features
            .enable(Feature::JsRepl)
            .expect("test config should allow feature update");
    });
    let TestPraxis {
        thread: praxis,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let call_id = "js-repl-view-image";
    let js_input = r#"
const fs = await import("node:fs/promises");
const path = await import("node:path");
const imagePath = path.join(praxis.tmpDir, "js-repl-view-image.png");
const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
  "base64"
);
await fs.writeFile(imagePath, png);
const out = await praxis.tool("view_image", { path: imagePath });
await praxis.emitImage(out);
"#;

    let first_response = sse(vec![
        ev_response_created("resp-1"),
        ev_custom_tool_call(call_id, "js_repl", js_input),
        ev_completed("resp-1"),
    ]);
    responses::mount_sse_once(&server, first_response).await;

    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    let mock = responses::mount_sse_once(&server, second_response).await;

    let session_model = session_configured.model.clone();
    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "use js_repl to write an image and attach it".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut tool_event = None;
    wait_for_event_with_timeout(
        &praxis,
        |event| match event {
            EventMsg::ViewImageToolCall(_) => {
                tool_event = Some(event.clone());
                false
            }
            EventMsg::TurnComplete(_) => true,
            _ => false,
        },
        Duration::from_secs(10),
    )
    .await;
    let tool_event = match tool_event {
        Some(EventMsg::ViewImageToolCall(event)) => event,
        other => panic!("expected ViewImageToolCall event, got {other:?}"),
    };
    assert!(
        tool_event.path.ends_with("js-repl-view-image.png"),
        "unexpected image path: {}",
        tool_event.path.display()
    );

    let req = mock.single_request();
    let body = req.body_json();
    assert_eq!(
        image_messages(&body).len(),
        0,
        "js_repl view_image should not inject a pending input image message"
    );

    let custom_output = req.custom_tool_call_output(call_id);
    let output_items = custom_output
        .get("output")
        .and_then(Value::as_array)
        .expect("custom_tool_call_output should be a content item array");
    let image_url = output_items
        .iter()
        .find_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("input_image"))
                .then(|| item.get("image_url").and_then(Value::as_str))
                .flatten()
        })
        .expect("image_url present in js_repl custom tool output");
    assert!(
        image_url.starts_with("data:image/png;base64,"),
        "expected png data URL, got {image_url}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn js_repl_view_image_requires_explicit_emit() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    #[allow(clippy::expect_used)]
    let mut builder = test_praxis().with_config(|config| {
        config
            .features
            .enable(Feature::JsRepl)
            .expect("test config should allow feature update");
    });
    let TestPraxis {
        thread: praxis,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let call_id = "js-repl-view-image-no-emit";
    let js_input = r#"
const fs = await import("node:fs/promises");
const path = await import("node:path");
const imagePath = path.join(praxis.tmpDir, "js-repl-view-image-no-emit.png");
const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
  "base64"
);
await fs.writeFile(imagePath, png);
const out = await praxis.tool("view_image", { path: imagePath });
console.log(out.type);
"#;

    let first_response = sse(vec![
        ev_response_created("resp-1"),
        ev_custom_tool_call(call_id, "js_repl", js_input),
        ev_completed("resp-1"),
    ]);
    responses::mount_sse_once(&server, first_response).await;

    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    let mock = responses::mount_sse_once(&server, second_response).await;

    let session_model = session_configured.model.clone();
    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "use js_repl to write an image but do not emit it".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut tool_event = None;
    wait_for_event_with_timeout(
        &praxis,
        |event| match event {
            EventMsg::ViewImageToolCall(_) => {
                tool_event = Some(event.clone());
                false
            }
            EventMsg::TurnComplete(_) => true,
            _ => false,
        },
        Duration::from_secs(10),
    )
    .await;
    let tool_event = match tool_event {
        Some(EventMsg::ViewImageToolCall(event)) => event,
        other => panic!("expected ViewImageToolCall event, got {other:?}"),
    };
    assert!(
        tool_event.path.ends_with("js-repl-view-image-no-emit.png"),
        "unexpected image path: {}",
        tool_event.path.display()
    );

    let req = mock.single_request();
    let custom_output = req.custom_tool_call_output(call_id);
    let output_items = custom_output.get("output").and_then(Value::as_array);
    assert!(
        output_items.is_none_or(|items| items
            .iter()
            .all(|item| item.get("type").and_then(Value::as_str) != Some("input_image"))),
        "nested view_image should not auto-populate js_repl output"
    );

    Ok(())
}

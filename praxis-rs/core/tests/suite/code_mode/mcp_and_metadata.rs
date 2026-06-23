use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_apply_patch_via_nested_tool() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let file_name = "code_mode_apply_patch.txt";
    let patch = format!(
        "*** Begin Patch\n*** Add File: {file_name}\n+hello from code_mode\n*** End Patch\n"
    );
    let code = format!("text(await tools.apply_patch({patch:?}));\n");

    let (test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to run apply_patch",
        &code,
        /*include_apply_patch*/ true,
    )
    .await?;

    let req = second_mock.single_request();
    let items = custom_tool_output_items(&req, "call-1");
    let (_, success) = req
        .custom_tool_call_output_content_and_success("call-1")
        .expect("custom tool output should be present");
    assert_ne!(
        success,
        Some(false),
        "exec apply_patch call failed unexpectedly: {items:?}"
    );
    assert_eq!(items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script completed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&items, /*index*/ 0),
    );
    assert_eq!(text_item(&items, /*index*/ 1), "{}");

    let file_path = test.cwd_path().join(file_name);
    assert_eq!(fs::read_to_string(&file_path)?, "hello from code_mode\n");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_print_structured_mcp_tool_result_fields() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
const { content, structuredContent, isError } = await tools.mcp__rmcp__echo({
  message: "ping",
});
text(
  `echo=${structuredContent?.echo ?? "missing"}\n` +
    `env=${structuredContent?.env ?? "missing"}\n` +
    `isError=${String(isError)}\n` +
    `contentLength=${content.length}`
);
"#;

    let (_test, second_mock) =
        run_code_mode_turn_with_rmcp(&server, "use exec to run the rmcp echo tool", code).await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec rmcp echo call failed unexpectedly: {output}"
    );
    assert_eq!(
        output,
        "echo=ECHOING: ping
env=propagated-env
isError=false
contentLength=0"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_exposes_mcp_tools_on_global_tools_object() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
const { content, structuredContent, isError } = await tools.mcp__rmcp__echo({
  message: "ping",
});
text(
  `hasEcho=${String(Object.keys(tools).includes("mcp__rmcp__echo"))}\n` +
    `echoType=${typeof tools.mcp__rmcp__echo}\n` +
    `echo=${structuredContent?.echo ?? "missing"}\n` +
    `isError=${String(isError)}\n` +
    `contentLength=${content.length}`
);
"#;

    let (_test, second_mock) =
        run_code_mode_turn_with_rmcp(&server, "use exec to inspect the global tools object", code)
            .await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec global rmcp access failed unexpectedly: {output}"
    );
    assert_eq!(
        output,
        "hasEcho=true
echoType=function
echo=ECHOING: ping
isError=false
contentLength=0"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_exposes_namespaced_mcp_tools_on_global_tools_object() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
text(JSON.stringify({
  hasExecCommand: typeof tools.exec_command === "function",
  hasNamespacedEcho: typeof tools.mcp__rmcp__echo === "function",
}));
"#;

    let (_test, second_mock) =
        run_code_mode_turn_with_rmcp(&server, "use exec to inspect the global tools object", code)
            .await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec global tools inspection failed unexpectedly: {output}"
    );

    let parsed: Value = serde_json::from_str(&output)?;
    assert_eq!(
        parsed,
        serde_json::json!({
            "hasExecCommand": !cfg!(windows),
            "hasNamespacedEcho": true,
        })
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_exposes_normalized_illegal_mcp_tool_names() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
const result = await tools.mcp__rmcp__echo_tool({ message: "ping" });
text(`echo=${result.structuredContent.echo}`);
"#;

    let (_test, second_mock) = run_code_mode_turn_with_rmcp(
        &server,
        "use exec to call a normalized rmcp tool name",
        code,
    )
    .await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec normalized rmcp tool call failed unexpectedly: {output}"
    );
    assert_eq!(output, "echo=ECHOING: ping");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_lists_global_scope_items() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
text(JSON.stringify(Object.getOwnPropertyNames(globalThis).sort()));
"#;

    let (_test, second_mock) =
        run_code_mode_turn_with_rmcp(&server, "use exec to inspect global scope", code).await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec global scope inspection failed unexpectedly: {output}"
    );
    let globals = serde_json::from_str::<Vec<String>>(&output)?;
    let globals = globals.into_iter().collect::<HashSet<_>>();
    let expected = [
        "AggregateError",
        "ALL_TOOLS",
        "Array",
        "ArrayBuffer",
        "AsyncDisposableStack",
        "Atomics",
        "BigInt",
        "BigInt64Array",
        "BigUint64Array",
        "Boolean",
        "DataView",
        "Date",
        "DisposableStack",
        "Error",
        "EvalError",
        "FinalizationRegistry",
        "Float16Array",
        "Float32Array",
        "Float64Array",
        "Function",
        "Infinity",
        "Int16Array",
        "Int32Array",
        "Int8Array",
        "Intl",
        "Iterator",
        "JSON",
        "Map",
        "Math",
        "NaN",
        "Number",
        "Object",
        "Promise",
        "Proxy",
        "RangeError",
        "ReferenceError",
        "Reflect",
        "RegExp",
        "Set",
        "SharedArrayBuffer",
        "String",
        "SuppressedError",
        "Symbol",
        "SyntaxError",
        "Temporal",
        "TypeError",
        "URIError",
        "Uint16Array",
        "Uint32Array",
        "Uint8Array",
        "Uint8ClampedArray",
        "WeakMap",
        "WeakRef",
        "WeakSet",
        "WebAssembly",
        "__codexContentItems",
        "add_content",
        "decodeURI",
        "decodeURIComponent",
        "encodeURI",
        "encodeURIComponent",
        "escape",
        "exit",
        "eval",
        "globalThis",
        "image",
        "isFinite",
        "isNaN",
        "load",
        "notify",
        "parseFloat",
        "parseInt",
        "store",
        "text",
        "tools",
        "undefined",
        "unescape",
        "yield_control",
    ];
    for g in &globals {
        assert!(
            expected.contains(&g.as_str()),
            "unexpected global {g} in {globals:?}"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_exports_all_tools_metadata_for_builtin_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
const tool = ALL_TOOLS.find(({ name }) => name === "view_image");
text(JSON.stringify(tool));
"#;

    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to inspect ALL_TOOLS",
        code,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec ALL_TOOLS lookup failed unexpectedly: {output}"
    );

    let parsed: Value = serde_json::from_str(
        &custom_tool_output_last_non_empty_text(&req, "call-1")
            .expect("exec ALL_TOOLS lookup should emit JSON"),
    )?;
    assert_eq!(
        parsed,
        serde_json::json!({
            "name": "view_image",
            "description": "View a local image from the filesystem (only use if given a full filepath by the user, and the image isn't already attached to the thread context within <image ...> tags).\n\nexec tool declaration:\n```ts\ndeclare const tools: { view_image(args: { path: string; }): Promise<{ detail: string | null; image_url: string; }>; };\n```",
        })
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_exports_all_tools_metadata_for_namespaced_mcp_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
const tool = ALL_TOOLS.find(
  ({ name }) => name === "mcp__rmcp__echo"
);
text(JSON.stringify(tool));
"#;

    let (_test, second_mock) =
        run_code_mode_turn_with_rmcp(&server, "use exec to inspect ALL_TOOLS", code).await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec ALL_TOOLS MCP lookup failed unexpectedly: {output}"
    );

    let parsed: Value = serde_json::from_str(
        &custom_tool_output_last_non_empty_text(&req, "call-1")
            .expect("exec ALL_TOOLS MCP lookup should emit JSON"),
    )?;
    assert_eq!(
        parsed,
        serde_json::json!({
            "name": "mcp__rmcp__echo",
            "description": "Echo back the provided message and include environment data.\n\nexec tool declaration:\n```ts\ndeclare const tools: { mcp__rmcp__echo(args: { env_var?: string; message: string; }): Promise<{ _meta?: unknown; content: Array<unknown>; isError?: boolean; structuredContent?: unknown; }>; };\n```",
        })
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_call_hidden_dynamic_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis().with_config(move |config| {
        let _ = config.features.enable(Feature::CodeMode);
    });
    let base_test = builder.build(&server).await?;
    let new_thread = base_test
        .thread_manager
        .start_thread_with_tools(
            base_test.config.clone(),
            vec![DynamicToolSpec {
                name: "hidden_dynamic_tool".to_string(),
                description: "A hidden dynamic tool.".to_string(),
                input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "city": { "type": "string" }
                        },
                    "required": ["city"],
                    "additionalProperties": false,
                }),
                defer_loading: true,
            }],
            /*persist_extended_history*/ false,
        )
        .await?;
    let mut test = base_test;
    test.thread = new_thread.thread;
    test.session_configured = new_thread.session_configured;

    let code = r#"
const tool = ALL_TOOLS.find(({ name }) => name === "hidden_dynamic_tool");
const out = await tools.hidden_dynamic_tool({ city: "Paris" });
text(
  JSON.stringify({
    name: tool?.name ?? null,
    description: tool?.description ?? null,
    out,
  })
);
"#;

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_custom_tool_call("call-1", "exec", code),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let second_mock = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "use exec to inspect and call hidden tools".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: test.session_configured.model.clone(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let turn_id = wait_for_event_match(&test.thread, |event| match event {
        EventMsg::TurnStarted(event) => Some(event.turn_id.clone()),
        _ => None,
    })
    .await;
    let request = wait_for_event_match(&test.thread, |event| match event {
        EventMsg::DynamicToolCallRequest(request) => Some(request.clone()),
        _ => None,
    })
    .await;
    assert_eq!(request.tool, "hidden_dynamic_tool");
    assert_eq!(request.arguments, serde_json::json!({ "city": "Paris" }));
    test.thread
        .submit(Op::DynamicToolResponse {
            id: request.call_id,
            response: DynamicToolResponse {
                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                    text: "hidden-ok".to_string(),
                }],
                success: true,
            },
        })
        .await?;
    wait_for_event(&test.thread, |event| match event {
        EventMsg::TurnComplete(event) => event.turn_id == turn_id,
        _ => false,
    })
    .await;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec hidden dynamic tool call failed unexpectedly: {output}"
    );

    let parsed: Value = serde_json::from_str(
        &custom_tool_output_last_non_empty_text(&req, "call-1")
            .expect("exec hidden dynamic tool lookup should emit JSON"),
    )?;
    assert_eq!(
        parsed.get("name"),
        Some(&Value::String("hidden_dynamic_tool".to_string()))
    );
    assert_eq!(
        parsed.get("out"),
        Some(&Value::String("hidden-ok".to_string()))
    );
    assert!(
        parsed
            .get("description")
            .and_then(Value::as_str)
            .is_some_and(|description| {
                description.contains("A hidden dynamic tool.")
                    && description.contains("declare const tools:")
                    && description.contains("hidden_dynamic_tool(args:")
            })
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_print_content_only_mcp_tool_result_fields() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
const { content, structuredContent, isError } = await tools.mcp__rmcp__image_scenario({
  scenario: "text_only",
  caption: "caption from mcp",
});
text(
  `firstType=${content[0]?.type ?? "missing"}\n` +
    `firstText=${content[0]?.text ?? "missing"}\n` +
    `structuredContent=${String(structuredContent ?? null)}\n` +
    `isError=${String(isError)}`
);
"#;

    let (_test, second_mock) = run_code_mode_turn_with_rmcp(
        &server,
        "use exec to run the rmcp image scenario tool",
        code,
    )
    .await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec rmcp image scenario call failed unexpectedly: {output}"
    );
    assert_eq!(
        output,
        "firstType=text
firstText=caption from mcp
structuredContent=null
isError=false"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_print_error_mcp_tool_result_fields() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let code = r#"
const { content, structuredContent, isError } = await tools.mcp__rmcp__echo({});
const firstText = content[0]?.text ?? "";
const mentionsMissingMessage =
  firstText.includes("missing field") && firstText.includes("message");
text(
  `isError=${String(isError)}\n` +
    `contentLength=${content.length}\n` +
    `mentionsMissingMessage=${String(mentionsMissingMessage)}\n` +
    `structuredContent=${String(structuredContent ?? null)}`
);
"#;

    let (_test, second_mock) =
        run_code_mode_turn_with_rmcp(&server, "use exec to call rmcp echo badly", code).await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec rmcp error call failed unexpectedly: {output}"
    );
    assert_eq!(
        output,
        "isError=true
contentLength=1
mentionsMissingMessage=true
structuredContent=null"
    );

    Ok(())
}

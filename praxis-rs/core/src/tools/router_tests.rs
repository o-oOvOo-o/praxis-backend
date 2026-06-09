use std::sync::Arc;

use crate::function_tool::FunctionCallError;
use crate::llm::runtime::LlmToolVisibilityPolicy;
use crate::praxis::make_session_and_context;
use crate::tools::context::ToolPayload;
use crate::turn_diff_tracker::TurnDiffTracker;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ApplyPatchToolType;

use super::ToolCall;
use super::ToolCallSource;
use super::ToolRouter;
use super::ToolRouterParams;
use super::freeform_input_from_function_arguments;

#[tokio::test]
async fn tool_visibility_policy_filters_model_visible_specs_only() -> anyhow::Result<()> {
    let (_session, turn) = make_session_and_context().await;
    let unfiltered = ToolRouter::from_config(
        &turn.tools_config,
        ToolRouterParams {
            mcp_tools: None,
            app_tools: None,
            discoverable_tools: None,
            dynamic_tools: turn.dynamic_tools.as_slice(),
            tool_visibility_policy: None,
        },
    );
    let unfiltered_specs = unfiltered.model_visible_specs();
    let visible_name = unfiltered_specs
        .first()
        .expect("test config should expose tools")
        .name()
        .to_string();
    let retained_internal_name = unfiltered_specs.get(1).map(|spec| spec.name().to_string());
    let policy = LlmToolVisibilityPolicy::from_tool_names(Some(&[visible_name.as_str()]), &[]);

    let filtered = ToolRouter::from_config(
        &turn.tools_config,
        ToolRouterParams {
            mcp_tools: None,
            app_tools: None,
            discoverable_tools: None,
            dynamic_tools: turn.dynamic_tools.as_slice(),
            tool_visibility_policy: Some(&policy),
        },
    );
    let visible_tools = filtered
        .model_visible_specs()
        .iter()
        .map(|spec| spec.name().to_string())
        .collect::<Vec<_>>();

    assert_eq!(visible_tools, vec![visible_name]);
    if let Some(retained_internal_name) = retained_internal_name {
        assert!(filtered.find_spec(&retained_internal_name).is_some());
    }
    Ok(())
}

#[tokio::test]
async fn js_repl_tools_only_blocks_direct_tool_calls() -> anyhow::Result<()> {
    let (session, mut turn) = make_session_and_context().await;
    turn.tools_config.js_repl_tools_only = true;

    let session = Arc::new(session);
    let turn = Arc::new(turn);
    let mcp_tools = session
        .services
        .mcp_connection_manager
        .read()
        .await
        .list_all_tools()
        .await;
    let app_tools = Some(mcp_tools.clone());
    let router = ToolRouter::from_config(
        &turn.tools_config,
        ToolRouterParams {
            mcp_tools: Some(
                mcp_tools
                    .into_iter()
                    .map(|(name, tool)| (name, tool.tool))
                    .collect(),
            ),
            app_tools,
            discoverable_tools: None,
            dynamic_tools: turn.dynamic_tools.as_slice(),
            tool_visibility_policy: None,
        },
    );

    let call = ToolCall {
        tool_name: "shell".to_string(),
        tool_namespace: None,
        call_id: "call-1".to_string(),
        payload: ToolPayload::Function {
            arguments: "{}".to_string(),
        },
    };
    let tracker = Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new()));
    let err = router
        .dispatch_tool_call_with_code_mode_result(
            session,
            turn,
            tracker,
            call,
            ToolCallSource::Direct,
        )
        .await
        .err()
        .expect("direct tool calls should be blocked");
    let FunctionCallError::RespondToModel(message) = err else {
        panic!("expected RespondToModel, got {err:?}");
    };
    assert!(message.contains("direct tool calls are disabled"));

    Ok(())
}

#[tokio::test]
async fn js_repl_tools_only_allows_js_repl_source_calls() -> anyhow::Result<()> {
    let (session, mut turn) = make_session_and_context().await;
    turn.tools_config.js_repl_tools_only = true;

    let session = Arc::new(session);
    let turn = Arc::new(turn);
    let mcp_tools = session
        .services
        .mcp_connection_manager
        .read()
        .await
        .list_all_tools()
        .await;
    let app_tools = Some(mcp_tools.clone());
    let router = ToolRouter::from_config(
        &turn.tools_config,
        ToolRouterParams {
            mcp_tools: Some(
                mcp_tools
                    .into_iter()
                    .map(|(name, tool)| (name, tool.tool))
                    .collect(),
            ),
            app_tools,
            discoverable_tools: None,
            dynamic_tools: turn.dynamic_tools.as_slice(),
            tool_visibility_policy: None,
        },
    );

    let call = ToolCall {
        tool_name: "shell".to_string(),
        tool_namespace: None,
        call_id: "call-2".to_string(),
        payload: ToolPayload::Function {
            arguments: "{}".to_string(),
        },
    };
    let tracker = Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new()));
    let err = router
        .dispatch_tool_call_with_code_mode_result(
            session,
            turn,
            tracker,
            call,
            ToolCallSource::JsRepl,
        )
        .await
        .err()
        .expect("shell call with empty args should fail");
    let message = err.to_string();
    assert!(
        !message.contains("direct tool calls are disabled"),
        "js_repl source should bypass direct-call policy gate"
    );

    Ok(())
}

#[tokio::test]
async fn build_tool_call_uses_namespace_for_registry_name() -> anyhow::Result<()> {
    let (session, _) = make_session_and_context().await;
    let session = Arc::new(session);
    let tool_name = "create_event".to_string();

    let call = ToolRouter::build_tool_call(
        &session,
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: tool_name.clone(),
            namespace: Some("mcp__praxis_apps__calendar".to_string()),
            arguments: "{}".to_string(),
            call_id: "call-namespace".to_string(),
        },
    )
    .await?
    .expect("function_call should produce a tool call");

    assert_eq!(call.tool_name, tool_name);
    assert_eq!(
        call.tool_namespace,
        Some("mcp__praxis_apps__calendar".to_string())
    );
    assert_eq!(call.call_id, "call-namespace");
    match call.payload {
        ToolPayload::Function { arguments } => {
            assert_eq!(arguments, "{}");
        }
        other => panic!("expected function payload, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn function_wrapped_freeform_keeps_function_output_type() -> anyhow::Result<()> {
    let (session, mut turn) = make_session_and_context().await;
    let cwd = tempfile::tempdir()?;
    turn.cwd = praxis_utils_absolute_path::AbsolutePathBuf::try_from(cwd.path().to_path_buf())?;
    turn.tools_config.apply_patch_tool_type = Some(ApplyPatchToolType::Freeform);

    let session = Arc::new(session);
    let turn = Arc::new(turn);
    let router = ToolRouter::from_config(
        &turn.tools_config,
        ToolRouterParams {
            mcp_tools: None,
            app_tools: None,
            discoverable_tools: None,
            dynamic_tools: turn.dynamic_tools.as_slice(),
            tool_visibility_policy: None,
        },
    );
    let patch = "*** Begin Patch\n*** Add File: done.txt\n+ok\n*** End Patch\n";
    let call = ToolCall {
        tool_name: "apply_patch".to_string(),
        tool_namespace: None,
        call_id: "call-common-apply-patch".to_string(),
        payload: ToolPayload::Function {
            arguments: serde_json::json!({ "input": patch }).to_string(),
        },
    };
    let tracker = Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new()));

    let result = router
        .dispatch_tool_call_with_code_mode_result(
            session,
            turn,
            tracker,
            call,
            ToolCallSource::Direct,
        )
        .await?;
    let response = result.into_response();

    match response {
        praxis_protocol::models::ResponseInputItem::FunctionCallOutput { call_id, output } => {
            assert_eq!(call_id, "call-common-apply-patch");
            assert!(output.success.unwrap_or(false));
            assert!(output.to_string().contains("Success. Updated"));
        }
        other => panic!("expected FunctionCallOutput, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(cwd.path().join("done.txt"))?,
        "ok\n"
    );

    Ok(())
}

#[test]
fn function_wrapped_freeform_uses_input_field() -> anyhow::Result<()> {
    let input = freeform_input_from_function_arguments(
        "exec",
        r#"{"input":"console.log('hello from common provider')"}"#,
    )?;

    assert_eq!(input, "console.log('hello from common provider')");
    Ok(())
}

#[test]
fn function_wrapped_apply_patch_accepts_patch_alias() -> anyhow::Result<()> {
    let patch = "*** Begin Patch\n*** End Patch\n";
    let input = freeform_input_from_function_arguments(
        "apply_patch",
        &serde_json::json!({ "patch": patch }).to_string(),
    )?;

    assert_eq!(input, patch);
    Ok(())
}

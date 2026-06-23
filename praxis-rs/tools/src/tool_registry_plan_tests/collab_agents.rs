use super::*;

#[test]
fn test_build_specs_collab_tools_enabled() {
    let model_info = model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::Collab);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );

    assert_contains_tool_names(
        &tools,
        &[
            "spawn_agent",
            "send_message",
            "assign_task",
            "wait_agent",
            "close_agent",
            "list_agents",
            "read_agent_artifact",
            "poll_runtime_commands",
            "submit_worker_request",
            "update_worker_request",
            "update_runtime_command",
        ],
    );
    assert_lacks_tool_name(&tools, "spawn_agents_on_csv");

    let spawn_agent = find_tool(&tools, "spawn_agent");
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = &spawn_agent.spec else {
        panic!("spawn_agent should be a function tool");
    };
    let JsonSchema::Object { properties, .. } = parameters else {
        panic!("spawn_agent should use object params");
    };
    assert!(properties.contains_key("fork_turns"));
    assert!(!properties.contains_key("fork_context"));
}

#[test]
fn test_build_specs_multi_agent_uses_task_names_and_hides_resume() {
    let model_info = model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::Collab);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );

    assert_contains_tool_names(
        &tools,
        &[
            "spawn_agent",
            "send_message",
            "assign_task",
            "wait_agent",
            "close_agent",
            "list_agents",
            "read_agent_artifact",
            "poll_runtime_commands",
            "submit_worker_request",
            "update_worker_request",
            "update_runtime_command",
        ],
    );

    let spawn_agent = find_tool(&tools, "spawn_agent");
    let ToolSpec::Function(ResponsesApiTool {
        parameters,
        output_schema,
        ..
    }) = &spawn_agent.spec
    else {
        panic!("spawn_agent should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("spawn_agent should use object params");
    };
    assert!(properties.contains_key("task_name"));
    assert!(properties.contains_key("title"));
    assert!(properties.contains_key("message"));
    assert!(properties.contains_key("fork_turns"));
    assert!(!properties.contains_key("items"));
    assert!(!properties.contains_key("fork_context"));
    assert_eq!(
        required.as_ref(),
        Some(&vec!["task_name".to_string(), "message".to_string()])
    );
    let output_schema = output_schema
        .as_ref()
        .expect("spawn_agent should define output schema");
    assert_eq!(
        output_schema["required"],
        json!([
            "agent_id",
            "task_name",
            "agent_base_name",
            "agent_title",
            "agent_display_name",
            "recommended_target",
            "next_action"
        ])
    );

    let send_message = find_tool(&tools, "send_message");
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = &send_message.spec else {
        panic!("send_message should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("send_message should use object params");
    };
    assert!(properties.contains_key("target"));
    assert!(!properties.contains_key("interrupt"));
    assert!(properties.contains_key("message"));
    assert!(!properties.contains_key("items"));
    assert_eq!(
        required.as_ref(),
        Some(&vec!["target".to_string(), "message".to_string()])
    );

    let assign_task = find_tool(&tools, "assign_task");
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = &assign_task.spec else {
        panic!("assign_task should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("assign_task should use object params");
    };
    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("objective"));
    assert!(properties.contains_key("message"));
    assert!(properties.contains_key("scope"));
    assert!(properties.contains_key("constraints"));
    assert!(properties.contains_key("acceptance_criteria"));
    assert!(properties.contains_key("artifact_refs"));
    assert!(properties.contains_key("required_capabilities"));
    assert!(properties.contains_key("required_resources"));
    assert!(properties.contains_key("token_budget"));
    assert!(properties.contains_key("priority"));
    assert!(properties.contains_key("exploratory"));
    assert!(!properties.contains_key("items"));
    assert_eq!(
        required.as_ref(),
        Some(&vec![
            "target".to_string(),
            "objective".to_string(),
            "scope".to_string()
        ])
    );

    let wait_agent = find_tool(&tools, "wait_agent");
    let ToolSpec::Function(ResponsesApiTool {
        parameters,
        output_schema,
        ..
    }) = &wait_agent.spec
    else {
        panic!("wait_agent should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("wait_agent should use object params");
    };
    assert!(!properties.contains_key("targets"));
    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("timeout_ms"));
    assert_eq!(required, &None);
    let output_schema = output_schema
        .as_ref()
        .expect("wait_agent should define output schema");
    assert_eq!(
        output_schema["properties"]["message"]["description"],
        json!("Brief wait summary.")
    );
    assert_eq!(
        output_schema["properties"]["source"]["enum"],
        json!(["mailbox", "agent_os", "target_status", "timeout"])
    );

    let list_agents = find_tool(&tools, "list_agents");
    let ToolSpec::Function(ResponsesApiTool {
        parameters,
        output_schema,
        ..
    }) = &list_agents.spec
    else {
        panic!("list_agents should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("list_agents should use object params");
    };
    assert!(properties.contains_key("path_prefix"));
    assert_eq!(required.as_ref(), None);
    let output_schema = output_schema
        .as_ref()
        .expect("list_agents should define output schema");
    assert_eq!(
        output_schema["properties"]["agents"]["items"]["required"],
        json!([
            "thread_id",
            "recommended_target",
            "next_action",
            "agent_name",
            "agent_base_name",
            "agent_title",
            "agent_display_name",
            "agent_role",
            "agent_status",
            "last_task_message"
        ])
    );
}

#[test]
fn test_build_specs_enable_fanout_enables_agent_jobs_and_collab_tools() {
    let model_info = model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::SpawnCsv);
    features.normalize_dependencies();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );

    assert_contains_tool_names(
        &tools,
        &[
            "spawn_agent",
            "send_message",
            "assign_task",
            "wait_agent",
            "close_agent",
            "list_agents",
            "read_agent_artifact",
            "poll_runtime_commands",
            "submit_worker_request",
            "update_worker_request",
            "update_runtime_command",
            "spawn_agents_on_csv",
        ],
    );
}

#[test]
fn test_build_specs_agent_job_worker_tools_enabled() {
    let model_info = model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::SpawnCsv);
    features.normalize_dependencies();
    features.enable(Feature::Sqlite);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::SubAgent(SubAgentSource::Other(
            "agent_job:test".to_string(),
        )),
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );

    assert_contains_tool_names(
        &tools,
        &[
            "spawn_agent",
            "send_message",
            "assign_task",
            "wait_agent",
            "close_agent",
            "list_agents",
            "read_agent_artifact",
            "poll_runtime_commands",
            "submit_worker_request",
            "update_worker_request",
            "update_runtime_command",
            "spawn_agents_on_csv",
            "report_agent_job_result",
        ],
    );
    assert_lacks_tool_name(&tools, "request_user_input");
}

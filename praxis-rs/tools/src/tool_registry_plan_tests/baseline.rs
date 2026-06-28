use super::*;

#[test]
fn test_full_toolset_specs_for_gpt5_praxis_unified_exec_web_search() {
    let model_info = model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Live),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(
        &config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );

    let mut actual = BTreeMap::new();
    let mut duplicate_names = Vec::new();
    for tool in &tools {
        let name = tool.name().to_string();
        if actual.insert(name.clone(), tool.spec.clone()).is_some() {
            duplicate_names.push(name);
        }
    }
    assert!(
        duplicate_names.is_empty(),
        "duplicate tool entries detected: {duplicate_names:?}"
    );

    let mut expected = BTreeMap::new();
    for spec in [
        create_exec_command_tool(CommandToolOptions {
            allow_login_shell: true,
            exec_permission_approvals_enabled: false,
        }),
        create_write_stdin_tool(),
        create_list_directory_tool(),
        create_update_plan_tool(),
        create_get_goal_tool(),
        create_create_goal_tool(),
        create_update_goal_tool(),
        request_user_input_tool_spec(/*default_mode_request_user_input*/ false),
        create_apply_patch_freeform_tool(),
        ToolSpec::WebSearch {
            external_web_access: Some(true),
            filters: None,
            user_location: None,
            search_context_size: None,
            search_content_types: None,
        },
        create_view_image_tool(ViewImageToolOptions {
            can_request_original_image_detail: config.can_request_original_image_detail,
        }),
    ] {
        expected.insert(spec.name().to_string(), spec);
    }
    let collab_specs = vec![
        create_spawn_agent_tool(spawn_agent_tool_options(&config)),
        create_send_message_tool(),
        create_assign_task_tool(),
        create_wait_agent_tool(wait_agent_timeout_options()),
        create_close_agent_tool(),
        create_list_agents_tool(),
        create_read_agent_artifact_tool(),
        create_poll_runtime_commands_tool(),
        create_submit_worker_request_tool(),
        create_update_worker_request_tool(),
        create_update_runtime_command_tool(),
    ];
    for spec in collab_specs {
        expected.insert(spec.name().to_string(), spec);
    }

    if config.exec_permission_approvals_enabled {
        let spec = create_request_permissions_tool(request_permissions_tool_description());
        expected.insert(spec.name().to_string(), spec);
    }

    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "tool name set mismatch"
    );

    for name in expected.keys() {
        let mut actual_spec = actual.get(name).expect("present").clone();
        let mut expected_spec = expected.get(name).expect("present").clone();
        strip_descriptions_tool(&mut actual_spec);
        strip_descriptions_tool(&mut expected_spec);
        assert_eq!(actual_spec, expected_spec, "spec mismatch for {name}");
    }
}

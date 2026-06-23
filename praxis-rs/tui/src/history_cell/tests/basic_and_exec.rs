use super::*;

#[test]
fn image_generation_call_renders_saved_path() {
    let saved_path = "file:///tmp/generated-image.png".to_string();
    let cell = new_image_generation_call(
        "call-image-generation".to_string(),
        Some("A tiny blue square".to_string()),
        Some(saved_path.clone()),
    );

    assert_eq!(
        render_lines(&cell.display_lines(/*width*/ 80)),
        vec![
            "• Generated Image:".to_string(),
            "  └ A tiny blue square".to_string(),
            format!("  └ Saved to: {saved_path}"),
        ],
    );
}

fn session_configured_event(model: &str) -> SessionConfiguredEvent {
    SessionConfiguredEvent {
        session_id: ThreadId::new(),
        forked_from_id: None,
        thread_name: None,
        model: model.to_string(),
        model_provider_id: "test-provider".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::Never,
        approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer::User,
        sandbox_policy: SandboxPolicy::new_read_only_policy(),
        cwd: PathBuf::from("/tmp/project").abs().to_path_buf(),
        reasoning_effort: None,
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path: Some(PathBuf::new()),
    }
}

#[test]
fn unified_exec_interaction_cell_renders_input() {
    let cell = new_unified_exec_interaction(Some("echo hello".to_string()), "ls\npwd".to_string());
    let lines = render_transcript(&cell);
    assert_eq!(
        lines,
        vec![
            "↳ Interacted with background terminal · echo hello",
            "  └ ls",
            "    pwd",
        ],
    );
}

#[test]
fn unified_exec_interaction_cell_renders_wait() {
    let cell = new_unified_exec_interaction(/*command_display*/ None, String::new());
    let lines = render_transcript(&cell);
    assert_eq!(lines, vec!["• Waited for background terminal"]);
}

#[test]
fn final_message_separator_hides_short_worked_label_and_includes_runtime_metrics() {
    let summary = RuntimeMetricsSummary {
        tool_calls: RuntimeMetricTotals {
            count: 3,
            duration_ms: 2_450,
        },
        api_calls: RuntimeMetricTotals {
            count: 2,
            duration_ms: 1_200,
        },
        streaming_events: RuntimeMetricTotals {
            count: 6,
            duration_ms: 900,
        },
        websocket_calls: RuntimeMetricTotals {
            count: 1,
            duration_ms: 700,
        },
        websocket_events: RuntimeMetricTotals {
            count: 4,
            duration_ms: 1_200,
        },
        responses_api_overhead_ms: 650,
        responses_api_inference_time_ms: 1_940,
        responses_api_engine_iapi_ttft_ms: 410,
        responses_api_engine_service_ttft_ms: 460,
        responses_api_engine_iapi_tbt_ms: 1_180,
        responses_api_engine_service_tbt_ms: 1_240,
        turn_ttft_ms: 0,
        turn_ttfm_ms: 0,
    };
    let cell = FinalMessageSeparator::new(Some(12), Some(summary));
    let rendered = render_lines(&cell.display_lines(/*width*/ 600));

    assert_eq!(rendered.len(), 1);
    assert!(!rendered[0].contains("Worked for"));
    assert!(rendered[0].contains("Local tools: 3 calls (2.5s)"));
    assert!(rendered[0].contains("Inference: 2 calls (1.2s)"));
    assert!(rendered[0].contains("WebSocket: 1 events send (700ms)"));
    assert!(rendered[0].contains("Streams: 6 events (900ms)"));
    assert!(rendered[0].contains("4 events received (1.2s)"));
    assert!(rendered[0].contains("Responses API overhead: 650ms"));
    assert!(rendered[0].contains("Responses API inference: 1.9s"));
    assert!(rendered[0].contains("TTFT: 410ms (iapi) 460ms (service)"));
    assert!(rendered[0].contains("TBT: 1.2s (iapi) 1.2s (service)"));
}

#[test]
fn final_message_separator_includes_worked_label_after_one_minute() {
    let cell = FinalMessageSeparator::new(Some(61), /*runtime_metrics*/ None);
    let rendered = render_lines(&cell.display_lines(/*width*/ 200));

    assert_eq!(rendered.len(), 1);
    assert!(rendered[0].contains("Worked for"));
}

#[test]
fn ps_output_empty_snapshot() {
    let cell = new_unified_exec_processes_output(Vec::new());
    let rendered = render_lines(&cell.display_lines(/*width*/ 60)).join("\n");
    insta::assert_snapshot!(rendered);
}

#[tokio::test]
async fn session_info_uses_availability_nux_tooltip_override() {
    let config = test_config().await;
    let tui_config = test_tui_config();
    let cell = new_session_info(
        &config,
        &tui_config,
        "gpt-5",
        session_configured_event("gpt-5"),
        /*is_first_event*/ false,
        Some("Model just became available".to_string()),
        Some(PlanType::Free),
        /*show_fast_status*/ false,
    );

    let rendered = render_transcript(&cell).join("\n");
    assert!(rendered.trim().is_empty());
}

#[tokio::test]
async fn session_info_availability_nux_tooltip_snapshot() {
    let mut config = test_config().await;
    config.cwd = PathBuf::from("/tmp/project").abs();
    let tui_config = test_tui_config();
    let cell = new_session_info(
        &config,
        &tui_config,
        "gpt-5",
        session_configured_event("gpt-5"),
        /*is_first_event*/ false,
        Some("Model just became available".to_string()),
        Some(PlanType::Free),
        /*show_fast_status*/ false,
    );

    let rendered = render_transcript(&cell).join("\n");
    assert!(rendered.trim().is_empty());
}

#[tokio::test]
async fn session_info_first_event_suppresses_tooltips_and_nux() {
    let config = test_config().await;
    let tui_config = test_tui_config();
    let cell = new_session_info(
        &config,
        &tui_config,
        "gpt-5",
        session_configured_event("gpt-5"),
        /*is_first_event*/ true,
        Some("Model just became available".to_string()),
        Some(PlanType::Free),
        /*show_fast_status*/ false,
    );

    let rendered = render_transcript(&cell).join("\n");
    assert!(!rendered.contains("Model just became available"));
    assert!(rendered.trim().is_empty());
}

#[tokio::test]
async fn session_info_hides_tooltips_when_disabled() {
    let config = test_config().await;
    let tui_config = TuiRuntimeConfig {
        show_tooltips: false,
        animations: false,
        ..Default::default()
    };
    let cell = new_session_info(
        &config,
        &tui_config,
        "gpt-5",
        session_configured_event("gpt-5"),
        /*is_first_event*/ false,
        Some("Model just became available".to_string()),
        Some(PlanType::Free),
        /*show_fast_status*/ false,
    );

    let rendered = render_transcript(&cell).join("\n");
    assert!(!rendered.contains("Model just became available"));
}

#[test]
fn ps_output_multiline_snapshot() {
    let cell = new_unified_exec_processes_output(vec![
        UnifiedExecProcessDetails {
            command_display: "echo hello\nand then some extra text".to_string(),
            recent_chunks: vec!["hello".to_string(), "done".to_string()],
        },
        UnifiedExecProcessDetails {
            command_display: "rg \"foo\" src".to_string(),
            recent_chunks: vec!["src/main.rs:12:foo".to_string()],
        },
    ]);
    let rendered = render_lines(&cell.display_lines(/*width*/ 40)).join("\n");
    insta::assert_snapshot!(rendered);
}

#[test]
fn ps_output_long_command_snapshot() {
    let cell = new_unified_exec_processes_output(vec![UnifiedExecProcessDetails {
        command_display: String::from(
            "rg \"foo\" src --glob '**/*.rs' --max-count 1000 --no-ignore --hidden --follow --glob '!target/**'",
        ),
        recent_chunks: vec!["searching...".to_string()],
    }]);
    let rendered = render_lines(&cell.display_lines(/*width*/ 36)).join("\n");
    insta::assert_snapshot!(rendered);
}

#[test]
fn ps_output_many_sessions_snapshot() {
    let cell = new_unified_exec_processes_output(
        (0..20)
            .map(|idx| UnifiedExecProcessDetails {
                command_display: format!("command {idx}"),
                recent_chunks: Vec::new(),
            })
            .collect(),
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 32)).join("\n");
    insta::assert_snapshot!(rendered);
}

#[test]
fn ps_output_chunk_leading_whitespace_snapshot() {
    let cell = new_unified_exec_processes_output(vec![UnifiedExecProcessDetails {
        command_display: "just fix".to_string(),
        recent_chunks: vec![
            "  indented first".to_string(),
            "    more indented".to_string(),
        ],
    }]);
    let rendered = render_lines(&cell.display_lines(/*width*/ 60)).join("\n");
    insta::assert_snapshot!(rendered);
}

#[test]
fn error_event_oversized_input_snapshot() {
    let cell = new_error_event(
        "Message exceeds the maximum length of 1048576 characters (1048577 provided).".to_string(),
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 120)).join("\n");
    insta::assert_snapshot!(rendered);
}

#[tokio::test]
async fn mcp_tools_output_masks_sensitive_values() {
    let mut config = test_config().await;
    let mut env = HashMap::new();
    env.insert("TOKEN".to_string(), "secret".to_string());
    let stdio_config = stdio_server_config("docs-server", vec![], Some(env), vec!["APP_TOKEN"]);
    let mut servers = config.mcp_servers.get().clone();
    servers.insert("docs".to_string(), stdio_config);

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer secret".to_string());
    let mut env_headers = HashMap::new();
    env_headers.insert("X-API-Key".to_string(), "API_KEY_ENV".to_string());
    let http_config = streamable_http_server_config(
        "https://example.com/mcp",
        Some("MCP_TOKEN"),
        Some(headers),
        Some(env_headers),
    );
    servers.insert("http".to_string(), http_config);
    config
        .mcp_servers
        .set(servers)
        .expect("test mcp servers should accept any configuration");

    let mut tools: HashMap<String, Tool> = HashMap::new();
    tools.insert(
        "mcp__docs__list".to_string(),
        Tool {
            description: None,
            name: "list".to_string(),
            title: None,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        },
    );
    tools.insert(
        "mcp__http__ping".to_string(),
        Tool {
            description: None,
            name: "ping".to_string(),
            title: None,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        },
    );

    let auth_statuses: HashMap<String, McpAuthStatus> = HashMap::new();
    let cell = new_mcp_tools_output(
        &config,
        tools,
        HashMap::new(),
        HashMap::new(),
        &auth_statuses,
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 120)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[tokio::test]
async fn mcp_tools_output_lists_tools_for_hyphenated_server_names() {
    let mut config = test_config().await;
    let mut servers = config.mcp_servers.get().clone();
    servers.insert(
        "some-server".to_string(),
        stdio_server_config("docs-server", vec!["--stdio"], /*env*/ None, vec![]),
    );
    config
        .mcp_servers
        .set(servers)
        .expect("test mcp servers should accept any configuration");

    let tools = HashMap::from([(
        "mcp__some_server__lookup".to_string(),
        Tool {
            description: None,
            name: "lookup".to_string(),
            title: None,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        },
    )]);

    let auth_statuses: HashMap<String, McpAuthStatus> = HashMap::new();
    let cell = new_mcp_tools_output(
        &config,
        tools,
        HashMap::new(),
        HashMap::new(),
        &auth_statuses,
    );
    let rendered = render_lines(&cell.display_lines(/*width*/ 120)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[tokio::test]
async fn mcp_tools_output_from_statuses_renders_status_only_servers() {
    let mut config = test_config().await;
    let mut plugin_docs =
        stdio_server_config("docs-server", vec!["--stdio"], /*env*/ None, vec![]);
    plugin_docs.enabled = false;
    plugin_docs.disabled_reason = Some(McpServerDisabledReason::Unknown);
    let servers = HashMap::from([("plugin_docs".to_string(), plugin_docs)]);
    config
        .mcp_servers
        .set(servers)
        .expect("test mcp servers should accept any configuration");

    let statuses = vec![McpServerStatus {
        name: "plugin_docs".to_string(),
        tools: HashMap::from([(
            "lookup".to_string(),
            Tool {
                description: None,
                name: "lookup".to_string(),
                title: None,
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
        )]),
        resources: Vec::new(),
        resource_templates: Vec::new(),
        auth_status: praxis_app_gateway_protocol::McpAuthStatus::Unsupported,
    }];

    let cell = new_mcp_tools_output_from_statuses(&config, &statuses);
    let rendered = render_lines(&cell.display_lines(/*width*/ 120)).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn empty_agent_message_cell_transcript() {
    let cell = AgentMessageCell::new(vec![Line::default()], /*is_first_line*/ false);
    assert_eq!(cell.transcript_lines(/*width*/ 80), vec![Line::from("  ")]);
    assert_eq!(cell.desired_transcript_height(/*width*/ 80), 1);
}

#[test]
fn prefixed_wrapped_history_cell_indents_wrapped_lines() {
    let summary = Line::from(vec![
        "You ".into(),
        "approved".bold(),
        " Praxis to run ".into(),
        "echo something really long to ensure wrapping happens".dim(),
        " this time".bold(),
    ]);
    let cell = PrefixedWrappedHistoryCell::new(summary, "✔ ".green(), "  ");
    let rendered = render_lines(&cell.display_lines(/*width*/ 24));
    assert_eq!(
        rendered,
        vec![
            "✔ You approved Praxis to".to_string(),
            "  run echo something".to_string(),
            "  really long to ensure".to_string(),
            "  wrapping happens this".to_string(),
            "  time".to_string(),
        ]
    );
}

#[test]
fn prefixed_wrapped_history_cell_does_not_split_url_like_token() {
    let url_like = "example.test/api/v1/projects/alpha-team/releases/2026-02-17/builds/1234567890";
    let cell = PrefixedWrappedHistoryCell::new(Line::from(url_like), "✔ ".green(), "  ");
    let rendered = render_lines(&cell.display_lines(/*width*/ 24));

    assert_eq!(
        rendered
            .iter()
            .filter(|line| line.contains(url_like))
            .count(),
        1,
        "expected full URL-like token in one rendered line, got: {rendered:?}"
    );
}

#[test]
fn unified_exec_interaction_cell_does_not_split_url_like_stdin_token() {
    let url_like = "example.test/api/v1/projects/alpha-team/releases/2026-02-17/builds/1234567890";
    let cell = UnifiedExecInteractionCell::new(Some("true".to_string()), url_like.to_string());
    let rendered = render_lines(&cell.display_lines(/*width*/ 24));

    assert_eq!(
        rendered
            .iter()
            .filter(|line| line.contains(url_like))
            .count(),
        1,
        "expected full URL-like token in one rendered line, got: {rendered:?}"
    );
}

#[test]
fn prefixed_wrapped_history_cell_height_matches_wrapped_rendering() {
    let url_like = "example.test/api/v1/projects/alpha-team/releases/2026-02-17/builds/1234567890/artifacts/reports/performance/summary/detail/with/a/very/long/path";
    let cell: Box<dyn HistoryCell> = Box::new(PrefixedWrappedHistoryCell::new(
        Line::from(url_like),
        "✔ ".green(),
        "  ",
    ));

    let width: u16 = 24;
    let logical_height = cell.display_lines(width).len() as u16;
    let wrapped_height = cell.desired_height(width);
    assert!(
        wrapped_height > logical_height,
        "expected wrapped height to exceed logical line count ({logical_height}), got {wrapped_height}"
    );

    let area = Rect::new(0, 0, width, wrapped_height);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    cell.render(area, &mut buf);

    let first_row = (0..area.width)
        .map(|x| {
            let symbol = buf[(x, 0)].symbol();
            if symbol.is_empty() {
                ' '
            } else {
                symbol.chars().next().unwrap_or(' ')
            }
        })
        .collect::<String>();
    assert!(
        first_row.contains("✔"),
        "expected first rendered row to keep the prefix visible, got: {first_row:?}"
    );
}

#[test]
fn unified_exec_interaction_cell_height_matches_wrapped_rendering() {
    let url_like = "example.test/api/v1/projects/alpha-team/releases/2026-02-17/builds/1234567890/artifacts/reports/performance/summary/detail/with/a/very/long/path";
    let cell: Box<dyn HistoryCell> = Box::new(UnifiedExecInteractionCell::new(
        Some("true".to_string()),
        url_like.to_string(),
    ));

    let width: u16 = 24;
    let logical_height = cell.display_lines(width).len() as u16;
    let wrapped_height = cell.desired_height(width);
    assert!(
        wrapped_height > logical_height,
        "expected wrapped height to exceed logical line count ({logical_height}), got {wrapped_height}"
    );

    let area = Rect::new(0, 0, width, wrapped_height);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    cell.render(area, &mut buf);

    let first_row = (0..area.width)
        .map(|x| {
            let symbol = buf[(x, 0)].symbol();
            if symbol.is_empty() {
                ' '
            } else {
                symbol.chars().next().unwrap_or(' ')
            }
        })
        .collect::<String>();
    assert!(
        first_row.contains("Interacted with"),
        "expected first rendered row to keep the header visible, got: {first_row:?}"
    );
}

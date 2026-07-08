use super::*;

pub(super) struct SendMessagePolicies<'a> {
    command_name: &'static str,
    experimental_api: bool,
    approval_policy: Option<AskForApproval>,
    sandbox_policy: Option<SandboxPolicy>,
    dynamic_tools: &'a Option<Vec<DynamicToolSpec>>,
}

pub(super) async fn send_message(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: String,
) -> Result<()> {
    let dynamic_tools = None;
    send_message_api_with_policies(
        endpoint,
        config_overrides,
        user_message,
        SendMessagePolicies {
            command_name: "send-message",
            experimental_api: false,
            approval_policy: None,
            sandbox_policy: None,
            dynamic_tools: &dynamic_tools,
        },
    )
    .await
}

pub async fn send_message_api(
    praxis_bin: &Path,
    config_overrides: &[String],
    user_message: String,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let endpoint = Endpoint::SpawnPraxis(praxis_bin.to_path_buf());
    send_message_api_endpoint(
        &endpoint,
        config_overrides,
        user_message,
        /*experimental_api*/ true,
        dynamic_tools,
    )
    .await
}

pub(super) async fn send_message_api_endpoint(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: String,
    experimental_api: bool,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    if dynamic_tools.is_some() && !experimental_api {
        bail!("--dynamic-tools requires --experimental-api for send-message-api");
    }

    send_message_api_with_policies(
        endpoint,
        config_overrides,
        user_message,
        SendMessagePolicies {
            command_name: "send-message-api",
            experimental_api,
            approval_policy: None,
            sandbox_policy: None,
            dynamic_tools,
        },
    )
    .await
}

pub(super) async fn trigger_zsh_fork_multi_cmd_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: Option<String>,
    min_approvals: usize,
    abort_on: Option<usize>,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    if let Some(abort_on) = abort_on
        && abort_on == 0
    {
        bail!("--abort-on must be >= 1 when provided");
    }

    let default_prompt = "Run this exact command using shell command execution without rewriting or splitting it: /usr/bin/true && /usr/bin/true";
    let message = user_message.unwrap_or_else(|| default_prompt.to_string());

    with_client(
        "trigger-zsh-fork-multi-cmd-approval",
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize()?;
            println!("< initialize response: {initialize:?}");

            let thread_response = client.thread_start(ThreadStartParams {
                dynamic_tools: dynamic_tools.clone(),
                ..Default::default()
            })?;
            println!("< thread/start response: {thread_response:?}");

            client.command_approval_behavior = match abort_on {
                Some(index) => CommandApprovalBehavior::AbortOn(index),
                None => CommandApprovalBehavior::AlwaysAccept,
            };
            client.command_approval_count = 0;
            client.command_approval_item_ids.clear();
            client.command_execution_statuses.clear();
            client.last_turn_status = None;

            let mut turn_params = TurnStartParams {
                thread_id: thread_response.thread.id.clone(),
                input: vec![ApiUserInput::Text {
                    text: message,
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            };
            turn_params.approval_policy = Some(AskForApproval::OnRequest);
            turn_params.sandbox_policy = Some(SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::FullAccess,
                network_access: false,
            });

            let turn_response = client.turn_start(turn_params)?;
            println!("< turn/start response: {turn_response:?}");
            client.stream_turn(&thread_response.thread.id, &turn_response.turn.id)?;

            if client.command_approval_count < min_approvals {
                bail!(
                    "expected at least {min_approvals} command approvals, got {}",
                    client.command_approval_count
                );
            }
            let mut approvals_per_item = std::collections::BTreeMap::new();
            for item_id in &client.command_approval_item_ids {
                *approvals_per_item.entry(item_id.clone()).or_insert(0usize) += 1;
            }
            let max_approvals_for_one_item =
                approvals_per_item.values().copied().max().unwrap_or(0);
            if max_approvals_for_one_item < min_approvals {
                bail!(
                    "expected at least {min_approvals} approvals for one command item, got max {max_approvals_for_one_item} with map {approvals_per_item:?}"
                );
            }

            let last_command_status = client.command_execution_statuses.last();
            if abort_on.is_none() {
                if last_command_status != Some(&CommandExecutionStatus::Completed) {
                    bail!("expected completed command execution, got {last_command_status:?}");
                }
                if client.last_turn_status != Some(TurnStatus::Completed) {
                    bail!(
                        "expected completed turn in all-accept flow, got {:?}",
                        client.last_turn_status
                    );
                }
            } else if last_command_status == Some(&CommandExecutionStatus::Completed) {
                bail!(
                    "expected non-completed command execution in mixed approval/decline flow, got {last_command_status:?}"
                );
            }

            println!(
                "[zsh-fork multi-approval summary] approvals={}, approvals_per_item={approvals_per_item:?}, command_statuses={:?}, turn_status={:?}",
                client.command_approval_count,
                client.command_execution_statuses,
                client.last_turn_status
            );

            Ok(())
        },
    )
    .await
}

pub(super) async fn resume_message_api(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    user_message: String,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    with_client("resume-message-api", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let resume_response = client.thread_resume(ThreadResumeParams {
            thread_id,
            dynamic_tools: dynamic_tools.clone(),
            ..Default::default()
        })?;
        println!("< thread/resume response: {resume_response:?}");

        let turn_response = client.turn_start(TurnStartParams {
            thread_id: resume_response.thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: user_message,
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })?;
        println!("< turn/start response: {turn_response:?}");

        client.stream_turn(&resume_response.thread.id, &turn_response.turn.id)?;

        Ok(())
    })
    .await
}

pub(super) async fn thread_resume_follow(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    with_client("thread-resume", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let resume_response = client.thread_resume(ThreadResumeParams {
            thread_id,
            dynamic_tools: dynamic_tools.clone(),
            ..Default::default()
        })?;
        println!("< thread/resume response: {resume_response:?}");
        println!("< streaming notifications until process is terminated");

        client.stream_notifications_forever()
    })
    .await
}

pub(super) async fn watch(endpoint: &Endpoint, config_overrides: &[String]) -> Result<()> {
    with_client("watch", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");
        println!("< streaming inbound messages until process is terminated");

        client.stream_notifications_forever()
    })
    .await
}

pub(super) async fn trigger_cmd_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: Option<String>,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let default_prompt =
        "Run `touch /tmp/should-trigger-approval` so I can confirm the file exists.";
    let message = user_message.unwrap_or_else(|| default_prompt.to_string());
    send_message_api_with_policies(
        endpoint,
        config_overrides,
        message,
        SendMessagePolicies {
            command_name: "trigger-cmd-approval",
            experimental_api: true,
            approval_policy: Some(AskForApproval::OnRequest),
            sandbox_policy: Some(SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::FullAccess,
                network_access: false,
            }),
            dynamic_tools,
        },
    )
    .await
}

pub(super) async fn trigger_patch_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: Option<String>,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let default_prompt =
        "Create a file named APPROVAL_DEMO.txt containing a short hello message using apply_patch.";
    let message = user_message.unwrap_or_else(|| default_prompt.to_string());
    send_message_api_with_policies(
        endpoint,
        config_overrides,
        message,
        SendMessagePolicies {
            command_name: "trigger-patch-approval",
            experimental_api: true,
            approval_policy: Some(AskForApproval::OnRequest),
            sandbox_policy: Some(SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::FullAccess,
                network_access: false,
            }),
            dynamic_tools,
        },
    )
    .await
}

pub(super) async fn no_trigger_cmd_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let prompt = "Run `touch should_not_trigger_approval.txt`";
    send_message_api_with_policies(
        endpoint,
        config_overrides,
        prompt.to_string(),
        SendMessagePolicies {
            command_name: "no-trigger-cmd-approval",
            experimental_api: true,
            approval_policy: None,
            sandbox_policy: None,
            dynamic_tools,
        },
    )
    .await
}

pub(super) async fn send_message_api_with_policies(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: String,
    policies: SendMessagePolicies<'_>,
) -> Result<()> {
    with_client(
        policies.command_name,
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize_with_experimental_api(policies.experimental_api)?;
            println!("< initialize response: {initialize:?}");

            let thread_response = client.thread_start(ThreadStartParams {
                dynamic_tools: policies.dynamic_tools.clone(),
                ..Default::default()
            })?;
            println!("< thread/start response: {thread_response:?}");
            let mut turn_params = TurnStartParams {
                thread_id: thread_response.thread.id.clone(),
                input: vec![ApiUserInput::Text {
                    text: user_message,
                    // Test client sends plain text without UI element ranges.
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            };
            turn_params.approval_policy = policies.approval_policy;
            turn_params.sandbox_policy = policies.sandbox_policy;

            let turn_response = client.turn_start(turn_params)?;
            println!("< turn/start response: {turn_response:?}");

            client.stream_turn(&thread_response.thread.id, &turn_response.turn.id)?;

            Ok(())
        },
    )
    .await
}

pub(super) async fn control_message_api(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: String,
    hold_seconds: u64,
) -> Result<()> {
    with_client(
        "control-message-api",
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize_with_experimental_api(true)?;
            println!("< initialize response: {initialize:?}");

            let thread_response = client.thread_start(ThreadStartParams {
                model: Some("deepseek-v4-pro".to_string()),
                model_provider: Some("deepseek".to_string()),
                cwd: Some("D:\\ghost1.0".to_string()),
                approval_policy: Some(AskForApproval::Never),
                sandbox: Some(SandboxMode::ReadOnly),
                ..Default::default()
            })?;
            println!("< thread/start response: {thread_response:?}");

            let controller = praxis_harness_controller();
            let control_response = client.thread_control_claim(ThreadControlClaimParams {
                thread_id: thread_response.thread.id.clone(),
                controller: controller.clone(),
                target_rank: Some(2),
                reason: Some(
                    "Praxis R0 is controlling this DeepSeek thread for Center observation"
                        .to_string(),
                ),
            })?;
            println!("< thread/control/claim response: {control_response:?}");

            let turn_response = client.turn_start(TurnStartParams {
                thread_id: thread_response.thread.id.clone(),
                input: vec![ApiUserInput::Text {
                    text: user_message,
                    text_elements: Vec::new(),
                }],
                model: Some("deepseek-v4-pro".to_string()),
                model_provider: Some("deepseek".to_string()),
                approval_policy: Some(AskForApproval::Never),
                sandbox_policy: Some(SandboxPolicy::ReadOnly {
                    access: ReadOnlyAccess::FullAccess,
                    network_access: false,
                }),
                ..Default::default()
            })?;
            println!("< turn/start response: {turn_response:?}");

            let stream_result =
                client.stream_turn(&thread_response.thread.id, &turn_response.turn.id);
            if hold_seconds > 0 {
                println!("[holding control for {hold_seconds}s]");
                thread::sleep(Duration::from_secs(hold_seconds));
            }
            let release_response = client.thread_control_release(ThreadControlReleaseParams {
                thread_id: thread_response.thread.id.clone(),
                controller: Some(controller),
            })?;
            println!("< thread/control/release response: {release_response:?}");

            stream_result?;

            Ok(())
        },
    )
    .await
}

pub(super) async fn control_release(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
) -> Result<()> {
    with_client("control-release", endpoint, config_overrides, |client| {
        let initialize = client.initialize_with_experimental_api(true)?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_control_release(ThreadControlReleaseParams {
            thread_id,
            controller: Some(praxis_harness_controller()),
        })?;
        println!("< thread/control/release response: {response:?}");

        Ok(())
    })
    .await
}

pub(super) async fn send_follow_up_api(
    endpoint: &Endpoint,
    config_overrides: &[String],
    first_message: String,
    follow_up_message: String,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    with_client("send-follow-up-api", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let thread_response = client.thread_start(ThreadStartParams {
            dynamic_tools: dynamic_tools.clone(),
            ..Default::default()
        })?;
        println!("< thread/start response: {thread_response:?}");

        let first_turn_params = TurnStartParams {
            thread_id: thread_response.thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: first_message,
                // Test client sends plain text without UI element ranges.
                text_elements: Vec::new(),
            }],
            ..Default::default()
        };
        let first_turn_response = client.turn_start(first_turn_params)?;
        println!("< turn/start response (initial): {first_turn_response:?}");
        client.stream_turn(&thread_response.thread.id, &first_turn_response.turn.id)?;

        let follow_up_params = TurnStartParams {
            thread_id: thread_response.thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: follow_up_message,
                // Test client sends plain text without UI element ranges.
                text_elements: Vec::new(),
            }],
            ..Default::default()
        };
        let follow_up_response = client.turn_start(follow_up_params)?;
        println!("< turn/start response (follow-up): {follow_up_response:?}");
        client.stream_turn(&thread_response.thread.id, &follow_up_response.turn.id)?;

        Ok(())
    })
    .await
}

pub(super) fn praxis_harness_controller() -> ThreadController {
    ThreadController {
        kind: ThreadControllerKind::External,
        id: "praxis-harness".to_string(),
        label: Some("Praxis harness".to_string()),
        rank: Some(0),
    }
}

pub(super) async fn test_login(
    endpoint: &Endpoint,
    config_overrides: &[String],
    device_code: bool,
) -> Result<()> {
    with_client("test-login", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let login_response = if device_code {
            client.login_account_chatgpt_device_code()?
        } else {
            client.login_account_chatgpt()?
        };
        println!("< account/login/start response: {login_response:?}");
        let login_id = match login_response {
            LoginAccountResponse::Chatgpt { login_id, auth_url } => {
                println!("Open the following URL in your browser to continue:\n{auth_url}");
                login_id
            }
            LoginAccountResponse::ChatgptDeviceCode {
                login_id,
                verification_url,
                user_code,
            } => {
                println!(
                    "Open the following URL and enter the code to continue:\n{verification_url}\n\nCode: {user_code}"
                );
                login_id
            }
            _ => bail!("expected chatgpt login response"),
        };

        let completion = client.wait_for_account_login_completion(&login_id)?;
        println!("< account/login/completed notification: {completion:?}");

        if completion.success {
            println!("Login succeeded.");
            Ok(())
        } else {
            bail!(
                "login failed: {}",
                completion
                    .error
                    .as_deref()
                    .unwrap_or("unknown error from account/login/completed")
            );
        }
    })
    .await
}

pub(super) async fn get_account_rate_limits(
    endpoint: &Endpoint,
    config_overrides: &[String],
) -> Result<()> {
    with_client(
        "get-account-rate-limits",
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize()?;
            println!("< initialize response: {initialize:?}");

            let response = client.get_account_rate_limits()?;
            println!("< account/rateLimits/read response: {response:?}");

            Ok(())
        },
    )
    .await
}

pub(super) async fn model_list(endpoint: &Endpoint, config_overrides: &[String]) -> Result<()> {
    with_client("model-list", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.model_list(ModelListParams::default())?;
        println!("< model/list response: {response:?}");

        Ok(())
    })
    .await
}

pub(super) async fn thread_list(
    endpoint: &Endpoint,
    config_overrides: &[String],
    limit: u32,
) -> Result<()> {
    with_client("thread-list", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_list(ThreadListParams {
            cursor: None,
            limit: Some(limit),
            sort_key: None,
            model_providers: None,
            source_kinds: None,
            archived: None,
            cwd: None,
            cwd_scope: None,
            search_term: None,
        })?;
        println!("< thread/list response: {response:?}");

        Ok(())
    })
    .await
}

pub(super) async fn with_client<T>(
    command_name: &'static str,
    endpoint: &Endpoint,
    config_overrides: &[String],
    f: impl FnOnce(&mut PraxisClient) -> Result<T>,
) -> Result<T> {
    let tracing = TestClientTracing::initialize(config_overrides).await?;
    let command_span = info_span!(
        "app_gateway_test_client.command",
        otel.kind = "client",
        otel.name = command_name,
        app_gateway_test_client.command = command_name,
    );
    let trace_summary = command_span.in_scope(|| TraceSummary::capture(tracing.traces_enabled));
    let result = command_span.in_scope(|| {
        let mut client = PraxisClient::connect(endpoint, config_overrides)?;
        f(&mut client)
    });
    print_trace_summary(&trace_summary);
    result
}

pub(super) fn thread_increment_elicitation(url: &str, thread_id: String) -> Result<()> {
    let endpoint = Endpoint::ConnectWs(url.to_string());
    let mut client = PraxisClient::connect(&endpoint, &[])?;

    let initialize = client.initialize()?;
    println!("< initialize response: {initialize:?}");

    let response =
        client.thread_increment_elicitation(ThreadIncrementElicitationParams { thread_id })?;
    println!("< thread/increment_elicitation response: {response:?}");

    Ok(())
}

pub(super) fn thread_decrement_elicitation(url: &str, thread_id: String) -> Result<()> {
    let endpoint = Endpoint::ConnectWs(url.to_string());
    let mut client = PraxisClient::connect(&endpoint, &[])?;

    let initialize = client.initialize()?;
    println!("< initialize response: {initialize:?}");

    let response =
        client.thread_decrement_elicitation(ThreadDecrementElicitationParams { thread_id })?;
    println!("< thread/decrement_elicitation response: {response:?}");

    Ok(())
}

pub(super) fn live_elicitation_timeout_pause(
    praxis_bin: Option<PathBuf>,
    url: Option<String>,
    config_overrides: &[String],
    model: String,
    workspace: PathBuf,
    script: Option<PathBuf>,
    hold_seconds: u64,
) -> Result<()> {
    if cfg!(windows) {
        bail!("live-elicitation-timeout-pause currently requires a POSIX shell");
    }
    if hold_seconds <= 10 {
        bail!("--hold-seconds must be greater than 10 to exceed the unified exec timeout");
    }

    let mut _background_server = None;
    let websocket_url = match (praxis_bin, url) {
        (Some(_), Some(_)) => bail!("--praxis-bin and --url are mutually exclusive"),
        (Some(praxis_bin), None) => {
            let server = BackgroundAppGateway::spawn(&praxis_bin, config_overrides)?;
            let websocket_url = server.url.clone();
            _background_server = Some(server);
            websocket_url
        }
        (None, Some(url)) => url,
        (None, None) => "ws://127.0.0.1:4222".to_string(),
    };

    let script_path = script.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join("live_elicitation_hold.sh")
    });
    if !script_path.is_file() {
        bail!("helper script not found: {}", script_path.display());
    }

    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace `{}`", workspace.display()))?;
    let app_gateway_test_client_bin = std::env::current_exe()
        .context("failed to resolve praxis-app-gateway-test-client binary path")?;
    let endpoint = Endpoint::ConnectWs(websocket_url.clone());
    let mut client = PraxisClient::connect(&endpoint, &[])?;

    let initialize = client.initialize()?;
    println!("< initialize response: {initialize:?}");

    let thread_response = client.thread_start(ThreadStartParams {
        model: Some(model),
        ..Default::default()
    })?;
    println!("< thread/start response: {thread_response:?}");

    let thread_id = thread_response.thread.id;
    let command = format!(
        "APP_GATEWAY_URL={} APP_GATEWAY_TEST_CLIENT_BIN={} PRAXIS_THREAD_ID={} ELICITATION_HOLD_SECONDS={} sh {}",
        shell_quote(&websocket_url),
        shell_quote(&app_gateway_test_client_bin.display().to_string()),
        shell_quote(&thread_id),
        hold_seconds,
        shell_quote(&script_path.display().to_string()),
    );
    let prompt = format!(
        "Use the `exec_command` tool exactly once. Set its `cmd` field to the exact shell command below. Do not rewrite it, do not split it, do not call any other tool, do not set `yield_time_ms`, and wait for the command to finish before replying.\n\n{command}\n\nAfter the command finishes, reply with exactly `DONE`."
    );

    let started_at = Instant::now();
    let turn_response = client.turn_start(TurnStartParams {
        thread_id: thread_id.clone(),
        input: vec![ApiUserInput::Text {
            text: prompt,
            text_elements: Vec::new(),
        }],
        approval_policy: Some(AskForApproval::Never),
        sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
        effort: Some(ReasoningEffort::High),
        cwd: Some(workspace),
        ..Default::default()
    })?;
    println!("< turn/start response: {turn_response:?}");

    let stream_result = client.stream_turn(&thread_id, &turn_response.turn.id);
    let elapsed = started_at.elapsed();

    let validation_result = (|| -> Result<()> {
        stream_result?;

        let helper_output = client
            .command_execution_outputs
            .iter()
            .find(|output| output.contains("[elicitation-hold]"))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("expected helper script markers in command output"))?;
        let minimum_elapsed = Duration::from_secs(hold_seconds.saturating_sub(1));

        if client.last_turn_status != Some(TurnStatus::Completed) {
            bail!(
                "expected completed turn, got {:?} (last error: {:?})",
                client.last_turn_status,
                client.last_turn_error_message
            );
        }
        if !client
            .command_execution_statuses
            .contains(&CommandExecutionStatus::Completed)
        {
            bail!(
                "expected a completed command execution, got {:?}",
                client.command_execution_statuses
            );
        }
        if !client.helper_done_seen || !helper_output.contains("[elicitation-hold] done") {
            bail!(
                "expected helper script completion marker in command output, got: {helper_output:?}"
            );
        }
        if !client.unexpected_items_before_helper_done.is_empty() {
            bail!(
                "turn started new items before helper completion: {:?}",
                client.unexpected_items_before_helper_done
            );
        }
        if client.turn_completed_before_helper_done {
            bail!("turn completed before helper script finished");
        }
        if elapsed < minimum_elapsed {
            bail!(
                "turn completed too quickly to prove timeout pause worked: elapsed={elapsed:?}, expected at least {minimum_elapsed:?}"
            );
        }

        Ok(())
    })();

    match client.thread_decrement_elicitation(ThreadDecrementElicitationParams {
        thread_id: thread_id.clone(),
    }) {
        Ok(response) => {
            println!("[cleanup] thread/decrement_elicitation response after harness: {response:?}");
        }
        Err(err) => {
            eprintln!("[cleanup] thread/decrement_elicitation ignored: {err:#}");
        }
    }

    validation_result?;

    println!(
        "[live elicitation timeout pause summary] thread_id={thread_id}, turn_id={}, elapsed={elapsed:?}, command_statuses={:?}",
        turn_response.turn.id, client.command_execution_statuses
    );

    Ok(())
}

pub(super) fn ensure_dynamic_tools_unused(
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
    command: &str,
) -> Result<()> {
    if dynamic_tools.is_some() {
        bail!(
            "dynamic tools are only supported for thread/start and thread/resume; remove --dynamic-tools for {command} or use send-message-api"
        );
    }
    Ok(())
}

pub(super) fn parse_dynamic_tools_arg(
    dynamic_tools: &Option<String>,
) -> Result<Option<Vec<DynamicToolSpec>>> {
    let Some(raw_arg) = dynamic_tools.as_deref() else {
        return Ok(None);
    };

    let raw_json = if let Some(path) = raw_arg.strip_prefix('@') {
        fs::read_to_string(Path::new(path))
            .with_context(|| format!("read dynamic tools file {path}"))?
    } else {
        raw_arg.to_string()
    };

    let value: Value = serde_json::from_str(&raw_json).context("parse dynamic tools JSON")?;
    let tools = match value {
        Value::Array(_) => serde_json::from_value(value).context("decode dynamic tools array")?,
        Value::Object(_) => vec![serde_json::from_value(value).context("decode dynamic tool")?],
        _ => bail!("dynamic tools JSON must be an object or array"),
    };

    Ok(Some(tools))
}

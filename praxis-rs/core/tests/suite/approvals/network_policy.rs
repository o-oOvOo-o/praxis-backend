use super::*;

#[tokio::test(flavor = "current_thread")]
async fn denying_network_policy_amendment_persists_policy_and_skips_future_network_prompt()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    fs::write(
        home.path().join("config.toml"),
        r#"default_permissions = "workspace"

[permissions.workspace.filesystem]
":minimal" = "read"

[permissions.workspace.network]
enabled = true
mode = "limited"
allow_local_binding = true
"#,
    )?;
    let approval_policy = AskForApproval::OnFailure;
    let sandbox_policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: Default::default(),
        network_access: true,
        exclude_tmpdir_env_var: false,
        exclude_slash_tmp: false,
    };
    let sandbox_policy_for_config = sandbox_policy.clone();
    let mut builder = test_praxis().with_home(home).with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
        let layers = config
            .config_layer_stack
            .get_layers(
                ConfigLayerStackOrdering::LowestPrecedenceFirst,
                /*include_disabled*/ true,
            )
            .into_iter()
            .cloned()
            .collect();
        let mut requirements = config.config_layer_stack.requirements().clone();
        requirements.network = Some(Sourced::new(
            NetworkConstraints {
                enabled: Some(true),
                allow_local_binding: Some(true),
                ..Default::default()
            },
            RequirementSource::CloudRequirements,
        ));
        let mut requirements_toml = config.config_layer_stack.requirements_toml().clone();
        requirements_toml.network = Some(NetworkRequirementsToml {
            enabled: Some(true),
            allow_local_binding: Some(true),
            ..Default::default()
        });
        config.config_layer_stack = ConfigLayerStack::new(layers, requirements, requirements_toml)
            .expect("rebuild config layer stack with network requirements");
    });
    let test = builder.build(&server).await?;
    assert!(
        test.config.managed_network_requirements_enabled(),
        "expected managed network requirements to be enabled"
    );
    assert!(
        test.config.permissions.network.is_some(),
        "expected managed network proxy config to be present"
    );
    test.session_configured
        .network_proxy
        .as_ref()
        .expect("expected runtime managed network proxy addresses");

    let call_id_first = "allow-network-first";
    // Use urllib without overriding proxy settings so managed-network sessions
    // continue to exercise the env-based proxy routing path under bubblewrap.
    let fetch_command = r#"python3 -c "import urllib.request; opener = urllib.request.build_opener(urllib.request.ProxyHandler()); print('OK:' + opener.open('http://praxis-network-test.invalid', timeout=30).read().decode(errors='replace'))""#
        .to_string();
    let first_event = shell_event(
        call_id_first,
        &fetch_command,
        /*timeout_ms*/ 30_000,
        SandboxPermissions::UseDefault,
    )?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-allow-network-1"),
            first_event,
            ev_completed("resp-allow-network-1"),
        ]),
    )
    .await;
    let first_results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-allow-network-1", "done"),
            ev_completed("resp-allow-network-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "allow-network-first",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let approval = loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .expect("timed out waiting for network approval request");
        let event = wait_for_event_with_timeout(
            &test.thread,
            |event| {
                matches!(
                    event,
                    EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
                )
            },
            remaining,
        )
        .await;
        match event {
            EventMsg::ExecApprovalRequest(approval) => {
                if approval.command.first().map(std::string::String::as_str)
                    == Some("network-access")
                {
                    break approval;
                }
                test.thread
                    .submit(Op::ExecApproval {
                        id: approval.effective_approval_id(),
                        turn_id: None,
                        decision: ReviewDecision::Approved,
                    })
                    .await?;
            }
            EventMsg::TurnComplete(_) => {
                panic!("expected network approval request before completion");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    };
    let network_context = approval
        .network_approval_context
        .clone()
        .expect("expected network approval context");
    assert_eq!(network_context.protocol, NetworkApprovalProtocol::Http);
    let expected_network_amendments = vec![
        NetworkPolicyAmendment {
            host: network_context.host.clone(),
            action: NetworkPolicyRuleAction::Allow,
        },
        NetworkPolicyAmendment {
            host: network_context.host.clone(),
            action: NetworkPolicyRuleAction::Deny,
        },
    ];
    assert_eq!(
        approval.proposed_network_policy_amendments,
        Some(expected_network_amendments.clone())
    );
    let deny_network_amendment = expected_network_amendments
        .into_iter()
        .find(|amendment| amendment.action == NetworkPolicyRuleAction::Deny)
        .expect("expected deny network policy amendment");

    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::NetworkPolicyAmendment {
                network_policy_amendment: deny_network_amendment.clone(),
            },
        })
        .await?;
    wait_for_completion(&test).await;

    let policy_path = test.home.path().join("rules").join("default.rules");
    let policy_contents = fs::read_to_string(&policy_path)?;
    let expected_rule = format!(
        r#"network_rule(host="{}", protocol="{}", decision="deny", justification="Deny {} access to {}")"#,
        deny_network_amendment.host,
        match network_context.protocol {
            NetworkApprovalProtocol::Http => "http",
            NetworkApprovalProtocol::Https => "https_connect",
            NetworkApprovalProtocol::Socks5Tcp => "socks5_tcp",
            NetworkApprovalProtocol::Socks5Udp => "socks5_udp",
        },
        match network_context.protocol {
            NetworkApprovalProtocol::Http => "http",
            NetworkApprovalProtocol::Https => "https_connect",
            NetworkApprovalProtocol::Socks5Tcp => "socks5_tcp",
            NetworkApprovalProtocol::Socks5Udp => "socks5_udp",
        },
        deny_network_amendment.host
    );
    assert!(
        policy_contents.contains(&expected_rule),
        "unexpected policy contents: {policy_contents}"
    );

    let first_output = parse_result(
        &first_results
            .single_request()
            .function_call_output(call_id_first),
    );
    Expectation::CommandFailure {
        output_contains: "",
    }
    .verify(&test, &first_output)?;

    let call_id_second = "allow-network-second";
    let second_event = shell_event(
        call_id_second,
        &fetch_command,
        /*timeout_ms*/ 30_000,
        SandboxPermissions::UseDefault,
    )?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-allow-network-3"),
            second_event,
            ev_completed("resp-allow-network-3"),
        ]),
    )
    .await;
    let second_results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-allow-network-2", "done"),
            ev_completed("resp-allow-network-4"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "allow-network-second",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .expect("timed out waiting for second turn completion");
        let event = wait_for_event_with_timeout(
            &test.thread,
            |event| {
                matches!(
                    event,
                    EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
                )
            },
            remaining,
        )
        .await;
        match event {
            EventMsg::ExecApprovalRequest(approval) => {
                if approval.command.first().map(std::string::String::as_str)
                    == Some("network-access")
                {
                    panic!(
                        "unexpected network approval request: {:?}",
                        approval.command
                    );
                }
                test.thread
                    .submit(Op::ExecApproval {
                        id: approval.effective_approval_id(),
                        turn_id: None,
                        decision: ReviewDecision::Approved,
                    })
                    .await?;
            }
            EventMsg::TurnComplete(_) => break,
            other => panic!("unexpected event: {other:?}"),
        }
    }

    let second_output = parse_result(
        &second_results
            .single_request()
            .function_call_output(call_id_second),
    );
    Expectation::CommandFailure {
        output_contains: "",
    }
    .verify(&test, &second_output)?;

    Ok(())
}

// todo(dylan) add ScenarioSpec support for rules

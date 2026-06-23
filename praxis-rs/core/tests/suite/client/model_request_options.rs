use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_configured_effort_in_request() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_model("gpt-5.1-codex")
        .with_config(|config| {
            config.model_reasoning_effort = Some(ReasoningEffort::Medium);
        })
        .build(&server)
        .await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|t| t.get("effort"))
            .and_then(|v| v.as_str()),
        Some("medium")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_no_effort_in_request() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_model("gpt-5.1-codex")
        .build(&server)
        .await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|t| t.get("effort"))
            .and_then(|v| v.as_str()),
        Some("medium")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_default_reasoning_effort_in_request_when_defined_by_model_info()
-> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } =
        test_praxis().with_model("gpt-5.1").build(&server).await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|t| t.get("effort"))
            .and_then(|v| v.as_str()),
        Some("medium")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_turn_collaboration_mode_overrides_model_and_effort() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis {
        thread: praxis,
        config,
        session_configured,
        ..
    } = test_praxis()
        .with_model("gpt-5.1-codex")
        .build(&server)
        .await?;

    let collaboration_mode = CollaborationMode {
        mode: ModeKind::Default,
        settings: Settings {
            model: "gpt-5.1".to_string(),
            reasoning_effort: Some(ReasoningEffort::High),
            developer_instructions: None,
        },
    };

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            cwd: config.cwd.to_path_buf(),
            approval_policy: config.permissions.approval_policy.value(),
            approvals_reviewer: None,
            sandbox_policy: config.permissions.sandbox_policy.get().clone(),
            model: session_configured.model.clone(),
            effort: Some(ReasoningEffort::Low),
            summary: Some(
                config
                    .model_reasoning_summary
                    .unwrap_or(ReasoningSummary::Auto),
            ),
            service_tier: None,
            collaboration_mode: Some(collaboration_mode),
            final_output_json_schema: None,
            personality: None,
        })
        .await?;

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request_body = resp_mock.single_request().body_json();
    assert_eq!(request_body["model"].as_str(), Some("gpt-5.1"));
    assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|t| t.get("effort"))
            .and_then(|v| v.as_str()),
        Some("high")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_reasoning_summary_is_sent() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.model_reasoning_summary = Some(ReasoningSummary::Concise);
        })
        .build(&server)
        .await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    pretty_assertions::assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("summary"))
            .and_then(|value| value.as_str()),
        Some("concise")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_turn_explicit_reasoning_summary_overrides_model_catalog_default() -> anyhow::Result<()>
{
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut model_catalog: ModelsResponse =
        serde_json::from_str(include_str!("../../models.json")).expect("valid models.json");
    let model = model_catalog
        .models
        .iter_mut()
        .find(|model| model.slug == "gpt-5.1")
        .expect("gpt-5.1 exists in bundled models.json");
    model.supports_reasoning_summaries = true;
    model.default_reasoning_summary = ReasoningSummary::Detailed;

    let TestPraxis {
        thread: praxis,
        config,
        session_configured,
        ..
    } = test_praxis()
        .with_model("gpt-5.1")
        .with_config(move |config| {
            config.model_catalog = Some(model_catalog);
        })
        .build(&server)
        .await?;

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            cwd: config.cwd.to_path_buf(),
            approval_policy: config.permissions.approval_policy.value(),
            approvals_reviewer: None,
            sandbox_policy: config.permissions.sandbox_policy.get().clone(),
            model: session_configured.model,
            effort: None,
            summary: Some(ReasoningSummary::Concise),
            service_tier: None,
            collaboration_mode: None,
            final_output_json_schema: None,
            personality: None,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request_body = resp_mock.single_request().body_json();

    pretty_assertions::assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("summary"))
            .and_then(|value| value.as_str()),
        Some("concise")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reasoning_summary_is_omitted_when_disabled() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.model_reasoning_summary = Some(ReasoningSummary::None);
        })
        .build(&server)
        .await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    pretty_assertions::assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("summary")),
        None
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reasoning_summary_none_overrides_model_catalog_default() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut model_catalog: ModelsResponse =
        serde_json::from_str(include_str!("../../models.json")).expect("valid models.json");
    let model = model_catalog
        .models
        .iter_mut()
        .find(|model| model.slug == "gpt-5.1")
        .expect("gpt-5.1 exists in bundled models.json");
    model.supports_reasoning_summaries = true;
    model.default_reasoning_summary = ReasoningSummary::Detailed;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_model("gpt-5.1")
        .with_config(move |config| {
            config.model_reasoning_summary = Some(ReasoningSummary::None);
            config.model_catalog = Some(model_catalog);
        })
        .build(&server)
        .await?;

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

    let request_body = resp_mock.single_request().body_json();
    pretty_assertions::assert_eq!(
        request_body
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("summary")),
        None
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_default_verbosity_in_request() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } =
        test_praxis().with_model("gpt-5.1").build(&server).await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert_eq!(
        request_body
            .get("text")
            .and_then(|t| t.get("verbosity"))
            .and_then(|v| v.as_str()),
        Some("low")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_verbosity_not_sent_for_models_without_support() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_model("gpt-5.1-codex")
        .with_config(|config| {
            config.model_verbosity = Some(Verbosity::High);
        })
        .build(&server)
        .await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert!(
        request_body
            .get("text")
            .and_then(|t| t.get("verbosity"))
            .is_none()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_verbosity_is_sent() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_model("gpt-5.1")
        .with_config(|config| {
            config.model_verbosity = Some(Verbosity::High);
        })
        .build(&server)
        .await?;

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

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert_eq!(
        request_body
            .get("text")
            .and_then(|t| t.get("verbosity"))
            .and_then(|v| v.as_str()),
        Some("high")
    );

    Ok(())
}

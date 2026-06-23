use super::*;

#[tokio::test]
async fn child_uses_parent_exec_policy_when_layer_stack_matches() {
    let (_home, parent_config) = test_config().await;
    let child_config = parent_config.clone();

    assert!(child_uses_parent_exec_policy(&parent_config, &child_config));
}

#[tokio::test]
async fn child_uses_parent_exec_policy_when_non_exec_policy_layers_differ() {
    let (_home, parent_config) = test_config().await;
    let mut child_config = parent_config.clone();
    let mut layers: Vec<_> = child_config
        .config_layer_stack
        .get_layers(
            ConfigLayerStackOrdering::LowestPrecedenceFirst,
            /*include_disabled*/ true,
        )
        .into_iter()
        .cloned()
        .collect();
    layers.push(ConfigLayerEntry::new(
        ConfigLayerSource::SessionFlags,
        TomlValue::Table(Default::default()),
    ));
    child_config.config_layer_stack = ConfigLayerStack::new(
        layers,
        child_config.config_layer_stack.requirements().clone(),
        child_config.config_layer_stack.requirements_toml().clone(),
    )
    .expect("config layer stack");

    assert!(child_uses_parent_exec_policy(&parent_config, &child_config));
}

#[tokio::test]
async fn child_does_not_use_parent_exec_policy_when_requirements_exec_policy_differs() {
    let (_home, parent_config) = test_config().await;
    let mut child_config = parent_config.clone();
    let mut requirements = ConfigRequirements {
        exec_policy: child_config
            .config_layer_stack
            .requirements()
            .exec_policy
            .clone(),
        ..ConfigRequirements::default()
    };
    let mut policy = Policy::empty();
    policy
        .add_prefix_rule(&["rm".to_string()], Decision::Forbidden)
        .expect("add prefix rule");
    requirements.exec_policy = Some(Sourced::new(
        RequirementsExecPolicy::new(policy),
        RequirementSource::Unknown,
    ));
    child_config.config_layer_stack = ConfigLayerStack::new(
        child_config
            .config_layer_stack
            .get_layers(
                ConfigLayerStackOrdering::LowestPrecedenceFirst,
                /*include_disabled*/ true,
            )
            .into_iter()
            .cloned()
            .collect(),
        requirements,
        child_config.config_layer_stack.requirements_toml().clone(),
    )
    .expect("config layer stack");

    assert!(!child_uses_parent_exec_policy(
        &parent_config,
        &child_config
    ));
}

#[tokio::test]
async fn returns_empty_policy_when_no_policy_files_exist() {
    let temp_dir = tempdir().expect("create temp dir");
    let config_stack = config_stack_for_dot_praxis_folder(temp_dir.path());

    let manager = ExecPolicyManager::load(&config_stack)
        .await
        .expect("manager result");
    let policy = manager.current();

    let commands = [vec!["rm".to_string()]];
    assert_eq!(
        Evaluation {
            decision: Decision::Allow,
            matched_rules: vec![RuleMatch::HeuristicsRuleMatch {
                command: vec!["rm".to_string()],
                decision: Decision::Allow
            }],
        },
        policy.check_multiple(commands.iter(), &|_| Decision::Allow)
    );
    assert!(!temp_dir.path().join(RULES_DIR_NAME).exists());
}

#[tokio::test]
async fn collect_policy_files_returns_empty_when_dir_missing() {
    let temp_dir = tempdir().expect("create temp dir");

    let policy_dir = temp_dir.path().join(RULES_DIR_NAME);
    let files = collect_policy_files(&policy_dir)
        .await
        .expect("collect policy files");

    assert!(files.is_empty());
}

#[tokio::test]
async fn format_exec_policy_error_with_source_renders_range() {
    let temp_dir = tempdir().expect("create temp dir");
    let config_stack = config_stack_for_dot_praxis_folder(temp_dir.path());
    let policy_dir = temp_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&policy_dir).expect("create policy dir");
    let broken_path = policy_dir.join("broken.rules");
    fs::write(
        &broken_path,
        r#"prefix_rule(
    pattern = ["tmux capture-pane"],
    decision = "allow",
    match = ["tmux capture-pane -p"],
)"#,
    )
    .expect("write broken policy file");

    let err = load_exec_policy(&config_stack)
        .await
        .expect_err("expected parse error");
    let rendered = format_exec_policy_error_with_source(&err);

    assert!(rendered.contains("broken.rules:1:"));
    assert!(rendered.contains("on or around line 1"));
}

#[test]
fn parse_starlark_line_from_message_extracts_path_and_line() {
    let parsed = parse_starlark_line_from_message(
        "/tmp/default.rules:143:1: starlark error: error: Parse error: unexpected new line",
    )
    .expect("parse should succeed");

    assert_eq!(parsed.0, PathBuf::from("/tmp/default.rules"));
    assert_eq!(parsed.1, 143);
}

#[test]
fn parse_starlark_line_from_message_rejects_zero_line() {
    let parsed = parse_starlark_line_from_message(
        "/tmp/default.rules:0:1: starlark error: error: Parse error: unexpected new line",
    );
    assert_eq!(parsed, None);
}

#[tokio::test]
async fn loads_policies_from_policy_subdirectory() {
    let temp_dir = tempdir().expect("create temp dir");
    let config_stack = config_stack_for_dot_praxis_folder(temp_dir.path());
    let policy_dir = temp_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&policy_dir).expect("create policy dir");
    fs::write(
        policy_dir.join("deny.rules"),
        r#"prefix_rule(pattern=["rm"], decision="forbidden")"#,
    )
    .expect("write policy file");

    let policy = load_exec_policy(&config_stack)
        .await
        .expect("policy result");
    let command = [vec!["rm".to_string()]];
    assert_eq!(
        Evaluation {
            decision: Decision::Forbidden,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: vec!["rm".to_string()],
                decision: Decision::Forbidden,
                resolved_program: None,
                justification: None,
            }],
        },
        policy.check_multiple(command.iter(), &|_| Decision::Allow)
    );
}

#[tokio::test]
async fn merges_requirements_exec_policy_network_rules() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;

    let mut requirements_exec_policy = Policy::empty();
    requirements_exec_policy.add_network_rule(
        "blocked.example.com",
        praxis_execpolicy::NetworkRuleProtocol::Https,
        Decision::Forbidden,
        /*justification*/ None,
    )?;

    let requirements = ConfigRequirements {
        exec_policy: Some(praxis_config::Sourced::new(
            praxis_config::RequirementsExecPolicy::new(requirements_exec_policy),
            praxis_config::RequirementSource::Unknown,
        )),
        ..ConfigRequirements::default()
    };
    let dot_praxis_folder = AbsolutePathBuf::from_absolute_path(temp_dir.path())?;
    let layer = ConfigLayerEntry::new(
        ConfigLayerSource::Project { dot_praxis_folder },
        TomlValue::Table(Default::default()),
    );
    let config_stack =
        ConfigLayerStack::new(vec![layer], requirements, ConfigRequirementsToml::default())?;

    let policy = load_exec_policy(&config_stack).await?;
    let (allowed, denied) = policy.compiled_network_domains();

    assert!(allowed.is_empty());
    assert_eq!(denied, vec!["blocked.example.com".to_string()]);
    Ok(())
}

#[tokio::test]
async fn preserves_host_executables_when_requirements_overlay_is_present() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let policy_dir = temp_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&policy_dir)?;
    let git_path = host_absolute_path(&["usr", "bin", "git"]);
    let git_path_literal = starlark_string(&git_path);
    fs::write(
        policy_dir.join("host.rules"),
        format!(
            r#"
host_executable(name = "git", paths = ["{git_path_literal}"])
"#
        ),
    )?;

    let mut requirements_exec_policy = Policy::empty();
    requirements_exec_policy.add_network_rule(
        "blocked.example.com",
        praxis_execpolicy::NetworkRuleProtocol::Https,
        Decision::Forbidden,
        /*justification*/ None,
    )?;

    let requirements = ConfigRequirements {
        exec_policy: Some(praxis_config::Sourced::new(
            praxis_config::RequirementsExecPolicy::new(requirements_exec_policy),
            praxis_config::RequirementSource::Unknown,
        )),
        ..ConfigRequirements::default()
    };
    let dot_praxis_folder = AbsolutePathBuf::from_absolute_path(temp_dir.path())?;
    let layer = ConfigLayerEntry::new(
        ConfigLayerSource::Project { dot_praxis_folder },
        TomlValue::Table(Default::default()),
    );
    let config_stack =
        ConfigLayerStack::new(vec![layer], requirements, ConfigRequirementsToml::default())?;

    let policy = load_exec_policy(&config_stack).await?;

    assert_eq!(
        policy
            .host_executables()
            .get("git")
            .expect("missing git host executable")
            .as_ref(),
        [AbsolutePathBuf::try_from(git_path)?]
    );
    Ok(())
}

#[tokio::test]
async fn ignores_policies_outside_policy_dir() {
    let temp_dir = tempdir().expect("create temp dir");
    let config_stack = config_stack_for_dot_praxis_folder(temp_dir.path());
    fs::write(
        temp_dir.path().join("root.rules"),
        r#"prefix_rule(pattern=["ls"], decision="prompt")"#,
    )
    .expect("write policy file");

    let policy = load_exec_policy(&config_stack)
        .await
        .expect("policy result");
    let command = [vec!["ls".to_string()]];
    assert_eq!(
        Evaluation {
            decision: Decision::Allow,
            matched_rules: vec![RuleMatch::HeuristicsRuleMatch {
                command: vec!["ls".to_string()],
                decision: Decision::Allow
            }],
        },
        policy.check_multiple(command.iter(), &|_| Decision::Allow)
    );
}

#[tokio::test]
async fn ignores_rules_from_untrusted_project_layers() -> anyhow::Result<()> {
    let project_dir = tempdir()?;
    let policy_dir = project_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&policy_dir)?;
    fs::write(
        policy_dir.join("untrusted.rules"),
        r#"prefix_rule(pattern=["ls"], decision="forbidden")"#,
    )?;

    let project_dot_praxis_folder = AbsolutePathBuf::from_absolute_path(project_dir.path())?;
    let layers = vec![ConfigLayerEntry::new_disabled(
        ConfigLayerSource::Project {
            dot_praxis_folder: project_dot_praxis_folder,
        },
        TomlValue::Table(Default::default()),
        "marked untrusted",
    )];
    let config_stack = ConfigLayerStack::new(
        layers,
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )?;

    let policy = load_exec_policy(&config_stack).await?;

    assert_eq!(
        Evaluation {
            decision: Decision::Allow,
            matched_rules: vec![RuleMatch::HeuristicsRuleMatch {
                command: vec!["ls".to_string()],
                decision: Decision::Allow,
            }],
        },
        policy.check_multiple([vec!["ls".to_string()]].iter(), &|_| Decision::Allow)
    );
    Ok(())
}

#[tokio::test]
async fn loads_policies_from_multiple_config_layers() -> anyhow::Result<()> {
    let user_dir = tempdir()?;
    let project_dir = tempdir()?;

    let user_policy_dir = user_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&user_policy_dir)?;
    fs::write(
        user_policy_dir.join("user.rules"),
        r#"prefix_rule(pattern=["rm"], decision="forbidden")"#,
    )?;

    let project_policy_dir = project_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&project_policy_dir)?;
    fs::write(
        project_policy_dir.join("project.rules"),
        r#"prefix_rule(pattern=["ls"], decision="prompt")"#,
    )?;

    let user_config_toml =
        AbsolutePathBuf::from_absolute_path(user_dir.path().join("config.toml"))?;
    let project_dot_praxis_folder = AbsolutePathBuf::from_absolute_path(project_dir.path())?;
    let layers = vec![
        ConfigLayerEntry::new(
            ConfigLayerSource::User {
                file: user_config_toml,
            },
            TomlValue::Table(Default::default()),
        ),
        ConfigLayerEntry::new(
            ConfigLayerSource::Project {
                dot_praxis_folder: project_dot_praxis_folder,
            },
            TomlValue::Table(Default::default()),
        ),
    ];
    let config_stack = ConfigLayerStack::new(
        layers,
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )?;

    let policy = load_exec_policy(&config_stack).await?;

    assert_eq!(
        Evaluation {
            decision: Decision::Forbidden,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: vec!["rm".to_string()],
                decision: Decision::Forbidden,
                resolved_program: None,
                justification: None,
            }],
        },
        policy.check_multiple([vec!["rm".to_string()]].iter(), &|_| Decision::Allow)
    );
    assert_eq!(
        Evaluation {
            decision: Decision::Prompt,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: vec!["ls".to_string()],
                decision: Decision::Prompt,
                resolved_program: None,
                justification: None,
            }],
        },
        policy.check_multiple([vec!["ls".to_string()]].iter(), &|_| Decision::Allow)
    );
    Ok(())
}

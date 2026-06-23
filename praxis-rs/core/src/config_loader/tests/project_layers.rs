use super::*;

#[tokio::test]
async fn project_layers_prefer_closest_cwd() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    tokio::fs::create_dir_all(nested.join(".praxis")).await?;
    tokio::fs::create_dir_all(project_root.join(".praxis")).await?;
    tokio::fs::write(project_root.join(".git"), "gitdir: here").await?;

    tokio::fs::write(
        project_root.join(".praxis").join(CONFIG_TOML_FILE),
        "foo = \"root\"\n",
    )
    .await?;
    tokio::fs::write(
        nested.join(".praxis").join(CONFIG_TOML_FILE),
        "foo = \"child\"\n",
    )
    .await?;

    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&praxis_home).await?;
    make_config_for_test(
        &praxis_home,
        &project_root,
        TrustLevel::Trusted,
        /*project_root_markers*/ None,
    )
    .await?;
    let cwd = AbsolutePathBuf::from_absolute_path(&nested)?;
    let layers = load_config_layers_state(
        &praxis_home,
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await?;

    let project_layers: Vec<_> = layers
        .layers_high_to_low()
        .into_iter()
        .filter_map(|layer| match &layer.name {
            super::ConfigLayerSource::Project { dot_praxis_folder } => Some(dot_praxis_folder),
            _ => None,
        })
        .collect();
    assert_eq!(project_layers.len(), 2);
    assert_eq!(
        project_layers[0].as_path(),
        nested.join(".praxis").as_path()
    );
    assert_eq!(
        project_layers[1].as_path(),
        project_root.join(".praxis").as_path()
    );

    let config = layers.effective_config();
    let foo = config
        .get("foo")
        .and_then(TomlValue::as_str)
        .expect("foo entry");
    assert_eq!(foo, "child");
    Ok(())
}

#[tokio::test]
async fn project_paths_resolve_relative_to_dot_praxis_and_override_in_order() -> std::io::Result<()>
{
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    tokio::fs::create_dir_all(project_root.join(".praxis")).await?;
    tokio::fs::create_dir_all(nested.join(".praxis")).await?;
    tokio::fs::write(project_root.join(".git"), "gitdir: here").await?;

    let root_cfg = r#"
model_instructions_file = "root.txt"
"#;
    let nested_cfg = r#"
model_instructions_file = "child.txt"
"#;
    tokio::fs::write(
        project_root.join(".praxis").join(CONFIG_TOML_FILE),
        root_cfg,
    )
    .await?;
    tokio::fs::write(nested.join(".praxis").join(CONFIG_TOML_FILE), nested_cfg).await?;
    tokio::fs::write(
        project_root.join(".praxis").join("root.txt"),
        "root instructions",
    )
    .await?;
    tokio::fs::write(
        nested.join(".praxis").join("child.txt"),
        "child instructions",
    )
    .await?;

    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&praxis_home).await?;
    make_config_for_test(
        &praxis_home,
        &project_root,
        TrustLevel::Trusted,
        /*project_root_markers*/ None,
    )
    .await?;

    let config = ConfigBuilder::default()
        .praxis_home(praxis_home)
        .harness_overrides(ConfigOverrides {
            cwd: Some(nested.clone()),
            ..ConfigOverrides::default()
        })
        .build()
        .await?;

    assert_eq!(
        config.base_instructions.as_deref(),
        Some("child instructions")
    );

    Ok(())
}

#[tokio::test]
async fn cli_override_model_instructions_file_sets_base_instructions() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&praxis_home).await?;
    tokio::fs::write(praxis_home.join(CONFIG_TOML_FILE), "").await?;

    let cwd = tmp.path().join("work");
    tokio::fs::create_dir_all(&cwd).await?;

    let instructions_path = tmp.path().join("instr.md");
    tokio::fs::write(&instructions_path, "cli override instructions").await?;

    let cli_overrides = vec![(
        "model_instructions_file".to_string(),
        TomlValue::String(instructions_path.to_string_lossy().to_string()),
    )];

    let config = ConfigBuilder::default()
        .praxis_home(praxis_home)
        .cli_overrides(cli_overrides)
        .harness_overrides(ConfigOverrides {
            cwd: Some(cwd),
            ..ConfigOverrides::default()
        })
        .build()
        .await?;

    assert_eq!(
        config.base_instructions.as_deref(),
        Some("cli override instructions")
    );

    Ok(())
}

#[tokio::test]
async fn project_layer_is_added_when_dot_praxis_exists_without_config_toml() -> std::io::Result<()>
{
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    tokio::fs::create_dir_all(&nested).await?;
    tokio::fs::create_dir_all(project_root.join(".praxis")).await?;
    tokio::fs::write(project_root.join(".git"), "gitdir: here").await?;

    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&praxis_home).await?;
    make_config_for_test(
        &praxis_home,
        &project_root,
        TrustLevel::Trusted,
        /*project_root_markers*/ None,
    )
    .await?;
    let cwd = AbsolutePathBuf::from_absolute_path(&nested)?;
    let layers = load_config_layers_state(
        &praxis_home,
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await?;

    let project_layers: Vec<_> = layers
        .layers_high_to_low()
        .into_iter()
        .filter(|layer| matches!(layer.name, super::ConfigLayerSource::Project { .. }))
        .collect();
    assert_eq!(
        vec![&ConfigLayerEntry {
            name: super::ConfigLayerSource::Project {
                dot_praxis_folder: AbsolutePathBuf::from_absolute_path(
                    project_root.join(".praxis")
                )?,
            },
            config: TomlValue::Table(toml::map::Map::new()),
            raw_toml: None,
            version: version_for_toml(&TomlValue::Table(toml::map::Map::new())),
            disabled_reason: None,
        }],
        project_layers
    );

    Ok(())
}

#[tokio::test]
async fn praxis_home_is_not_loaded_as_project_layer_from_home_dir() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let home_dir = tmp.path().join("home");
    let praxis_home = home_dir.join(".praxis");
    tokio::fs::create_dir_all(&praxis_home).await?;
    tokio::fs::write(praxis_home.join(CONFIG_TOML_FILE), "foo = \"user\"\n").await?;

    let cwd = AbsolutePathBuf::from_absolute_path(&home_dir)?;
    let layers = load_config_layers_state(
        &praxis_home,
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await?;

    let project_layers: Vec<_> = layers
        .get_layers(
            super::ConfigLayerStackOrdering::HighestPrecedenceFirst,
            /*include_disabled*/ true,
        )
        .into_iter()
        .filter(|layer| matches!(layer.name, super::ConfigLayerSource::Project { .. }))
        .collect();
    let expected: Vec<&ConfigLayerEntry> = Vec::new();
    assert_eq!(expected, project_layers);
    assert_eq!(
        layers.effective_config().get("foo"),
        Some(&TomlValue::String("user".to_string()))
    );

    Ok(())
}

#[tokio::test]
async fn praxis_home_within_project_tree_is_not_double_loaded() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    let project_dot_praxis = project_root.join(".praxis");
    let nested_dot_praxis = nested.join(".praxis");

    tokio::fs::create_dir_all(&nested_dot_praxis).await?;
    tokio::fs::create_dir_all(project_root.join(".git")).await?;
    tokio::fs::write(
        nested_dot_praxis.join(CONFIG_TOML_FILE),
        "foo = \"child\"\n",
    )
    .await?;

    tokio::fs::create_dir_all(&project_dot_praxis).await?;
    make_config_for_test(
        &project_dot_praxis,
        &project_root,
        TrustLevel::Trusted,
        /*project_root_markers*/ None,
    )
    .await?;
    let user_config_path = project_dot_praxis.join(CONFIG_TOML_FILE);
    let user_config_contents = tokio::fs::read_to_string(&user_config_path).await?;
    tokio::fs::write(
        &user_config_path,
        format!("foo = \"user\"\n{user_config_contents}"),
    )
    .await?;

    let cwd = AbsolutePathBuf::from_absolute_path(&nested)?;
    let layers = load_config_layers_state(
        &project_dot_praxis,
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await?;

    let project_layers: Vec<_> = layers
        .get_layers(
            super::ConfigLayerStackOrdering::HighestPrecedenceFirst,
            /*include_disabled*/ true,
        )
        .into_iter()
        .filter(|layer| matches!(layer.name, super::ConfigLayerSource::Project { .. }))
        .collect();

    let child_config: TomlValue = toml::from_str("foo = \"child\"\n").expect("parse child config");
    assert_eq!(
        vec![&ConfigLayerEntry {
            name: super::ConfigLayerSource::Project {
                dot_praxis_folder: AbsolutePathBuf::from_absolute_path(&nested_dot_praxis)?,
            },
            config: child_config.clone(),
            raw_toml: None,
            version: version_for_toml(&child_config),
            disabled_reason: None,
        }],
        project_layers
    );
    assert_eq!(
        layers.effective_config().get("foo"),
        Some(&TomlValue::String("child".to_string()))
    );

    Ok(())
}

#[tokio::test]
async fn project_layers_disabled_when_untrusted_or_unknown() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    tokio::fs::create_dir_all(nested.join(".praxis")).await?;
    tokio::fs::write(
        nested.join(".praxis").join(CONFIG_TOML_FILE),
        "foo = \"child\"\n",
    )
    .await?;

    let cwd = AbsolutePathBuf::from_absolute_path(&nested)?;

    let praxis_home_untrusted = tmp.path().join("home_untrusted");
    tokio::fs::create_dir_all(&praxis_home_untrusted).await?;
    make_config_for_test(
        &praxis_home_untrusted,
        &project_root,
        TrustLevel::Untrusted,
        /*project_root_markers*/ None,
    )
    .await?;
    let untrusted_config_path = praxis_home_untrusted.join(CONFIG_TOML_FILE);
    let untrusted_config_contents = tokio::fs::read_to_string(&untrusted_config_path).await?;
    tokio::fs::write(
        &untrusted_config_path,
        format!("foo = \"user\"\n{untrusted_config_contents}"),
    )
    .await?;

    let layers_untrusted = load_config_layers_state(
        &praxis_home_untrusted,
        Some(cwd.clone()),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await?;
    let project_layers_untrusted: Vec<_> = layers_untrusted
        .get_layers(
            super::ConfigLayerStackOrdering::HighestPrecedenceFirst,
            /*include_disabled*/ true,
        )
        .into_iter()
        .filter(|layer| matches!(layer.name, super::ConfigLayerSource::Project { .. }))
        .collect();
    assert_eq!(project_layers_untrusted.len(), 1);
    assert!(
        project_layers_untrusted[0].disabled_reason.is_some(),
        "expected untrusted project layer to be disabled"
    );
    assert_eq!(
        project_layers_untrusted[0].config.get("foo"),
        Some(&TomlValue::String("child".to_string()))
    );
    assert_eq!(
        layers_untrusted.effective_config().get("foo"),
        Some(&TomlValue::String("user".to_string()))
    );

    let praxis_home_unknown = tmp.path().join("home_unknown");
    tokio::fs::create_dir_all(&praxis_home_unknown).await?;
    tokio::fs::write(
        praxis_home_unknown.join(CONFIG_TOML_FILE),
        "foo = \"user\"\n",
    )
    .await?;

    let layers_unknown = load_config_layers_state(
        &praxis_home_unknown,
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await?;
    let project_layers_unknown: Vec<_> = layers_unknown
        .get_layers(
            super::ConfigLayerStackOrdering::HighestPrecedenceFirst,
            /*include_disabled*/ true,
        )
        .into_iter()
        .filter(|layer| matches!(layer.name, super::ConfigLayerSource::Project { .. }))
        .collect();
    assert_eq!(project_layers_unknown.len(), 1);
    assert!(
        project_layers_unknown[0].disabled_reason.is_some(),
        "expected unknown-trust project layer to be disabled"
    );
    assert_eq!(
        project_layers_unknown[0].config.get("foo"),
        Some(&TomlValue::String("child".to_string()))
    );
    assert_eq!(
        layers_unknown.effective_config().get("foo"),
        Some(&TomlValue::String("user".to_string()))
    );

    Ok(())
}

#[tokio::test]
async fn cli_override_can_update_project_local_mcp_server_when_project_is_trusted()
-> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    let dot_praxis = project_root.join(".praxis");
    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&nested).await?;
    tokio::fs::create_dir_all(&dot_praxis).await?;
    tokio::fs::create_dir_all(&praxis_home).await?;
    tokio::fs::write(project_root.join(".git"), "gitdir: here").await?;
    tokio::fs::write(
        dot_praxis.join(CONFIG_TOML_FILE),
        r#"
[mcp_servers.sentry]
url = "https://mcp.sentry.dev/mcp"
enabled = false
"#,
    )
    .await?;
    make_config_for_test(
        &praxis_home,
        &project_root,
        TrustLevel::Trusted,
        /*project_root_markers*/ None,
    )
    .await?;

    let config = ConfigBuilder::default()
        .praxis_home(praxis_home)
        .cli_overrides(vec![(
            "mcp_servers.sentry.enabled".to_string(),
            TomlValue::Boolean(true),
        )])
        .fallback_cwd(Some(nested))
        .build()
        .await?;

    let server = config
        .mcp_servers
        .get()
        .get("sentry")
        .expect("trusted project MCP server should load");
    assert!(server.enabled);

    Ok(())
}

#[tokio::test]
async fn cli_override_for_disabled_project_local_mcp_server_returns_invalid_transport()
-> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    let dot_praxis = project_root.join(".praxis");
    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&nested).await?;
    tokio::fs::create_dir_all(&dot_praxis).await?;
    tokio::fs::create_dir_all(&praxis_home).await?;
    tokio::fs::write(project_root.join(".git"), "gitdir: here").await?;
    tokio::fs::write(
        dot_praxis.join(CONFIG_TOML_FILE),
        r#"
[mcp_servers.sentry]
url = "https://mcp.sentry.dev/mcp"
enabled = false
"#,
    )
    .await?;

    let err = ConfigBuilder::default()
        .praxis_home(praxis_home)
        .cli_overrides(vec![(
            "mcp_servers.sentry.enabled".to_string(),
            TomlValue::Boolean(true),
        )])
        .fallback_cwd(Some(nested))
        .build()
        .await
        .expect_err("untrusted project layer should not provide MCP transport");

    assert!(
        err.to_string().contains("invalid transport")
            && err.to_string().contains("mcp_servers.sentry"),
        "unexpected error: {err}"
    );

    Ok(())
}

#[tokio::test]
async fn invalid_project_config_ignored_when_untrusted_or_unknown() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    tokio::fs::create_dir_all(nested.join(".praxis")).await?;
    tokio::fs::write(project_root.join(".git"), "gitdir: here").await?;
    tokio::fs::write(nested.join(".praxis").join(CONFIG_TOML_FILE), "foo =").await?;

    let cwd = AbsolutePathBuf::from_absolute_path(&nested)?;
    let cases = [
        ("untrusted", Some(TrustLevel::Untrusted)),
        ("unknown", None),
    ];

    for (name, trust_level) in cases {
        let praxis_home = tmp.path().join(format!("home_{name}"));
        tokio::fs::create_dir_all(&praxis_home).await?;
        let config_path = praxis_home.join(CONFIG_TOML_FILE);

        if let Some(trust_level) = trust_level {
            make_config_for_test(
                &praxis_home,
                &project_root,
                trust_level,
                /*project_root_markers*/ None,
            )
            .await?;
            let config_contents = tokio::fs::read_to_string(&config_path).await?;
            tokio::fs::write(&config_path, format!("foo = \"user\"\n{config_contents}")).await?;
        } else {
            tokio::fs::write(&config_path, "foo = \"user\"\n").await?;
        }

        let layers = load_config_layers_state(
            &praxis_home,
            Some(cwd.clone()),
            &[] as &[(String, TomlValue)],
            LoaderOverrides::default(),
            CloudRequirementsLoader::default(),
        )
        .await?;
        let project_layers: Vec<_> = layers
            .get_layers(
                super::ConfigLayerStackOrdering::HighestPrecedenceFirst,
                /*include_disabled*/ true,
            )
            .into_iter()
            .filter(|layer| matches!(layer.name, super::ConfigLayerSource::Project { .. }))
            .collect();
        assert_eq!(
            project_layers.len(),
            1,
            "expected one project layer for {name}"
        );
        assert!(
            project_layers[0].disabled_reason.is_some(),
            "expected {name} project layer to be disabled"
        );
        assert_eq!(
            project_layers[0].config,
            TomlValue::Table(toml::map::Map::new())
        );
        assert_eq!(
            layers.effective_config().get("foo"),
            Some(&TomlValue::String("user".to_string()))
        );
    }

    Ok(())
}

#[tokio::test]
async fn cli_overrides_with_relative_paths_do_not_break_trust_check() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    tokio::fs::create_dir_all(&nested).await?;
    tokio::fs::write(project_root.join(".git"), "gitdir: here").await?;

    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&praxis_home).await?;
    make_config_for_test(
        &praxis_home,
        &project_root,
        TrustLevel::Trusted,
        /*project_root_markers*/ None,
    )
    .await?;

    let cwd = AbsolutePathBuf::from_absolute_path(&nested)?;
    let cli_overrides = vec![(
        "model_instructions_file".to_string(),
        TomlValue::String("relative.md".to_string()),
    )];

    load_config_layers_state(
        &praxis_home,
        Some(cwd),
        &cli_overrides,
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await?;

    Ok(())
}

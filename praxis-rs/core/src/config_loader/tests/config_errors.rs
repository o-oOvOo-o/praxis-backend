use super::*;

#[tokio::test]
async fn cli_overrides_resolve_relative_paths_against_cwd() -> std::io::Result<()> {
    let praxis_home = tempdir().expect("tempdir");
    let cwd_dir = tempdir().expect("tempdir");
    let cwd_path = cwd_dir.path().to_path_buf();

    let config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .cli_overrides(vec![(
            "log_dir".to_string(),
            TomlValue::String("run-logs".to_string()),
        )])
        .harness_overrides(ConfigOverrides {
            cwd: Some(cwd_path.clone()),
            ..Default::default()
        })
        .build()
        .await?;

    let expected = AbsolutePathBuf::resolve_path_against_base("run-logs", cwd_path)?;
    assert_eq!(config.log_dir, expected.to_path_buf());
    Ok(())
}

#[tokio::test]
async fn returns_config_error_for_invalid_user_config_toml() {
    let tmp = tempdir().expect("tempdir");
    let contents = "model = \"gpt-4\"\ninvalid = [";
    let config_path = tmp.path().join(CONFIG_TOML_FILE);
    std::fs::write(&config_path, contents).expect("write config");

    let cwd = AbsolutePathBuf::try_from(tmp.path()).expect("cwd");
    let err = load_config_layers_state(
        tmp.path(),
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::default(),
    )
    .await
    .expect_err("expected error");

    let config_error = config_error_from_io(&err);
    let expected_toml_error = toml::from_str::<TomlValue>(contents).expect_err("parse error");
    let expected_config_error =
        super::config_error_from_toml(&config_path, contents, expected_toml_error);
    assert_eq!(config_error, &expected_config_error);
}

#[tokio::test]
async fn returns_config_error_for_invalid_managed_config_toml() {
    let tmp = tempdir().expect("tempdir");
    let managed_path = tmp.path().join("managed_config.toml");
    let contents = "model = \"gpt-4\"\ninvalid = [";
    std::fs::write(&managed_path, contents).expect("write managed config");

    let overrides = LoaderOverrides {
        managed_config_path: Some(managed_path.clone()),
        ..Default::default()
    };

    let cwd = AbsolutePathBuf::try_from(tmp.path()).expect("cwd");
    let err = load_config_layers_state(
        tmp.path(),
        Some(cwd),
        &[] as &[(String, TomlValue)],
        overrides,
        CloudRequirementsLoader::default(),
    )
    .await
    .expect_err("expected error");

    let config_error = config_error_from_io(&err);
    let expected_toml_error = toml::from_str::<TomlValue>(contents).expect_err("parse error");
    let expected_config_error =
        super::config_error_from_toml(&managed_path, contents, expected_toml_error);
    assert_eq!(config_error, &expected_config_error);
}

#[tokio::test]
async fn returns_config_error_for_schema_error_in_user_config() {
    let tmp = tempdir().expect("tempdir");
    let contents = "model_context_window = \"not_a_number\"";
    let config_path = tmp.path().join(CONFIG_TOML_FILE);
    std::fs::write(&config_path, contents).expect("write config");

    let err = ConfigBuilder::default()
        .praxis_home(tmp.path().to_path_buf())
        .fallback_cwd(Some(tmp.path().to_path_buf()))
        .build()
        .await
        .expect_err("expected error");

    let config_error = config_error_from_io(&err);
    let _guard = praxis_utils_absolute_path::AbsolutePathBufGuard::new(tmp.path());
    let expected_config_error =
        praxis_config::config_error_from_typed_toml::<ConfigToml>(&config_path, contents)
            .expect("schema error");
    assert_eq!(config_error, &expected_config_error);
}

#[test]
fn schema_error_points_to_feature_value() {
    let tmp = tempdir().expect("tempdir");
    let contents = "[features]\ncollaboration_modes = \"true\"";
    let config_path = tmp.path().join(CONFIG_TOML_FILE);
    std::fs::write(&config_path, contents).expect("write config");

    let _guard = praxis_utils_absolute_path::AbsolutePathBufGuard::new(tmp.path());
    let error = praxis_config::config_error_from_typed_toml::<ConfigToml>(&config_path, contents)
        .expect("schema error");

    let value_line = contents.lines().nth(1).expect("value line");
    let value_column = value_line.find("\"true\"").expect("value") + 1;
    assert_eq!(error.range.start.line, 2);
    assert_eq!(error.range.start.column, value_column);
}

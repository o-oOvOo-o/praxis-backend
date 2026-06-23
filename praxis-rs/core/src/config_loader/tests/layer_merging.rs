use super::*;

#[tokio::test]
async fn merges_managed_config_layer_on_top() {
    let tmp = tempdir().expect("tempdir");
    let managed_path = tmp.path().join("managed_config.toml");

    std::fs::write(
        tmp.path().join(CONFIG_TOML_FILE),
        r#"foo = 1

[nested]
value = "base"
"#,
    )
    .expect("write base");
    std::fs::write(
        &managed_path,
        r#"foo = 2

[nested]
value = "managed_config"
extra = true
"#,
    )
    .expect("write managed config");

    let overrides = LoaderOverrides {
        managed_config_path: Some(managed_path),
        #[cfg(target_os = "macos")]
        managed_preferences_base64: None,
        macos_managed_config_requirements_base64: None,
    };

    let cwd = AbsolutePathBuf::try_from(tmp.path()).expect("cwd");
    let state = load_config_layers_state(
        tmp.path(),
        Some(cwd),
        &[] as &[(String, TomlValue)],
        overrides,
        CloudRequirementsLoader::default(),
    )
    .await
    .expect("load config");
    let loaded = state.effective_config();
    let table = loaded.as_table().expect("top-level table expected");

    assert_eq!(table.get("foo"), Some(&TomlValue::Integer(2)));
    let nested = table
        .get("nested")
        .and_then(|v| v.as_table())
        .expect("nested");
    assert_eq!(
        nested.get("value"),
        Some(&TomlValue::String("managed_config".to_string()))
    );
    assert_eq!(nested.get("extra"), Some(&TomlValue::Boolean(true)));
}

#[tokio::test]
async fn returns_empty_when_all_layers_missing() {
    let tmp = tempdir().expect("tempdir");
    let managed_path = tmp.path().join("managed_config.toml");

    let overrides = LoaderOverrides {
        managed_config_path: Some(managed_path),
        #[cfg(target_os = "macos")]
        // Force managed preferences to resolve as empty so this test does not
        // inherit non-empty machine-specific managed state.
        managed_preferences_base64: Some(String::new()),
        macos_managed_config_requirements_base64: None,
    };

    let cwd = AbsolutePathBuf::try_from(tmp.path()).expect("cwd");
    let layers = load_config_layers_state(
        tmp.path(),
        Some(cwd),
        &[] as &[(String, TomlValue)],
        overrides,
        CloudRequirementsLoader::default(),
    )
    .await
    .expect("load layers");
    let user_layer = layers
        .get_user_layer()
        .expect("expected a user layer even when PRAXIS_HOME/config.toml does not exist");
    assert_eq!(
        &ConfigLayerEntry {
            name: super::ConfigLayerSource::User {
                file: AbsolutePathBuf::resolve_path_against_base(CONFIG_TOML_FILE, tmp.path())
                    .expect("resolve user config.toml path")
            },
            config: TomlValue::Table(toml::map::Map::new()),
            raw_toml: None,
            version: version_for_toml(&TomlValue::Table(toml::map::Map::new())),
            disabled_reason: None,
        },
        user_layer,
    );
    assert_eq!(
        user_layer.config,
        TomlValue::Table(toml::map::Map::new()),
        "expected empty config for user layer when config.toml does not exist"
    );

    let binding = layers.effective_config();
    let base_table = binding.as_table().expect("base table expected");
    assert!(
        base_table.is_empty(),
        "expected empty base layer when configs missing"
    );
    let num_system_layers = layers
        .layers_high_to_low()
        .iter()
        .filter(|layer| matches!(layer.name, super::ConfigLayerSource::System { .. }))
        .count();
    assert_eq!(
        num_system_layers, 1,
        "system layer should always be present"
    );

    #[cfg(not(target_os = "macos"))]
    {
        let effective = layers.effective_config();
        let table = effective.as_table().expect("top-level table expected");
        assert!(
            table.is_empty(),
            "expected empty table when configs missing"
        );
    }
}

#[tokio::test]
async fn falls_back_to_shared_praxis_user_config_for_default_praxis_home() -> std::io::Result<()> {
    let praxis_home = tempdir()?;
    let shared_praxis_home = tempdir()?;
    tokio::fs::write(
        shared_praxis_home.path().join(CONFIG_TOML_FILE),
        "model = \"shared-praxis-model\"\n",
    )
    .await?;

    let resolved = super::resolve_user_config_toml_file_with_shared_home(
        praxis_home.path(),
        Some(shared_praxis_home.path()),
    )
    .await?;

    assert_eq!(
        resolved,
        AbsolutePathBuf::resolve_path_against_base(CONFIG_TOML_FILE, shared_praxis_home.path())?
    );
    Ok(())
}

use super::*;

#[tokio::test]
async fn returns_empty_when_all_layers_missing() {
    let tmp = tempdir().expect("tempdir");

    let cwd = AbsolutePathBuf::try_from(tmp.path()).expect("cwd");
    let layers = load_config_layers_state(
        tmp.path(),
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
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

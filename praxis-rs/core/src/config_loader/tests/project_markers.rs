use super::*;

#[tokio::test]
async fn project_root_markers_supports_alternate_markers() -> std::io::Result<()> {
    let tmp = tempdir()?;
    let project_root = tmp.path().join("project");
    let nested = project_root.join("child");
    tokio::fs::create_dir_all(project_root.join(".praxis")).await?;
    tokio::fs::create_dir_all(nested.join(".praxis")).await?;
    tokio::fs::write(project_root.join(".hg"), "hg").await?;
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
        Some(vec![".hg".to_string()]),
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

    let merged = layers.effective_config();
    let foo = merged
        .get("foo")
        .and_then(TomlValue::as_str)
        .expect("foo entry");
    assert_eq!(foo, "child");

    Ok(())
}

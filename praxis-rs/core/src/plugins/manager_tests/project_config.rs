use super::*;

#[test]
fn load_plugins_ignores_project_config_files() {
    let praxis_home = TempDir::new().unwrap();
    let project_root = praxis_home.path().join("project");
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{"name":"sample"}"#,
    );
    write_file(
        &project_root.join(".praxis/config.toml"),
        &plugin_config_toml(/*enabled*/ true, /*plugins_feature_enabled*/ true),
    );

    let stack = ConfigLayerStack::new(
        vec![ConfigLayerEntry::new(
            ConfigLayerSource::Project {
                dot_praxis_folder: AbsolutePathBuf::try_from(project_root.join(".praxis")).unwrap(),
            },
            toml::from_str(&plugin_config_toml(
                /*enabled*/ true, /*plugins_feature_enabled*/ true,
            ))
            .expect("project config should parse"),
        )],
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )
    .expect("config layer stack should build");

    let outcome = load_plugins_from_layer_stack(
        &stack,
        &PluginStore::new(praxis_home.path().to_path_buf()),
        Some(Product::Praxis),
    );

    assert_eq!(outcome, PluginLoadOutcome::default());
}

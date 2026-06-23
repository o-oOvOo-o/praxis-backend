use super::*;

#[tokio::test]
async fn featured_plugin_ids_for_config_uses_restriction_product_query_param() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        &tmp.path().join(CONFIG_TOML_FILE),
        r#"[features]
plugins = true
"#,
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/featured"))
        .and(query_param("platform", "chat"))
        .and(header("authorization", "Bearer Access Token"))
        .and(header("chatgpt-account-id", "account_id"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"["chat-plugin"]"#))
        .mount(&server)
        .await;

    let mut config = load_config(tmp.path(), tmp.path()).await;
    config.chatgpt_base_url = format!("{}/backend-api/", server.uri());
    let manager = PluginsManager::new_with_restriction_product(
        tmp.path().to_path_buf(),
        Some(Product::Chatgpt),
    );

    let featured_plugin_ids = manager
        .featured_plugin_ids_for_config(
            &config,
            Some(&OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing()),
        )
        .await
        .unwrap();

    assert_eq!(featured_plugin_ids, vec!["chat-plugin".to_string()]);
}

#[tokio::test]
async fn featured_plugin_ids_for_config_defaults_query_param_to_codex() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        &tmp.path().join(CONFIG_TOML_FILE),
        r#"[features]
plugins = true
"#,
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/featured"))
        .and(query_param("platform", "codex"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"["praxis-plugin"]"#))
        .mount(&server)
        .await;

    let mut config = load_config(tmp.path(), tmp.path()).await;
    config.chatgpt_base_url = format!("{}/backend-api/", server.uri());
    let manager = PluginsManager::new_with_restriction_product(
        tmp.path().to_path_buf(),
        /*restriction_product*/ None,
    );

    let featured_plugin_ids = manager
        .featured_plugin_ids_for_config(&config, /*auth*/ None)
        .await
        .unwrap();

    assert_eq!(featured_plugin_ids, vec!["praxis-plugin".to_string()]);
}

#[test]
fn refresh_curated_plugin_cache_replaces_existing_local_version_with_sha() {
    let tmp = tempfile::tempdir().unwrap();
    let curated_root = curated_plugins_repo_path(tmp.path());
    write_curated_marketplace(&curated_root, &["slack"]);
    write_curated_plugin_sha(tmp.path(), TEST_CURATED_PLUGIN_SHA);
    let plugin_id = PluginId::new(
        "slack".to_string(),
        OPENAI_CURATED_MARKETPLACE_NAME.to_string(),
    )
    .unwrap();
    write_plugin(
        &tmp.path().join("plugins/cache/openai-curated"),
        "slack/local",
        "slack",
    );

    assert!(
        refresh_curated_plugin_cache(tmp.path(), TEST_CURATED_PLUGIN_SHA, &[plugin_id])
            .expect("cache refresh should succeed")
    );

    assert!(
        !tmp.path()
            .join("plugins/cache/openai-curated/slack/local")
            .exists()
    );
    assert!(
        tmp.path()
            .join(format!(
                "plugins/cache/openai-curated/slack/{TEST_CURATED_PLUGIN_SHA}"
            ))
            .is_dir()
    );
}

#[test]
fn refresh_curated_plugin_cache_reinstalls_missing_configured_plugin_with_current_sha() {
    let tmp = tempfile::tempdir().unwrap();
    let curated_root = curated_plugins_repo_path(tmp.path());
    write_curated_marketplace(&curated_root, &["slack"]);
    write_curated_plugin_sha(tmp.path(), TEST_CURATED_PLUGIN_SHA);
    let plugin_id = PluginId::new(
        "slack".to_string(),
        OPENAI_CURATED_MARKETPLACE_NAME.to_string(),
    )
    .unwrap();

    assert!(
        refresh_curated_plugin_cache(tmp.path(), TEST_CURATED_PLUGIN_SHA, &[plugin_id])
            .expect("cache refresh should recreate missing configured plugin")
    );

    assert!(
        tmp.path()
            .join(format!(
                "plugins/cache/openai-curated/slack/{TEST_CURATED_PLUGIN_SHA}"
            ))
            .is_dir()
    );
}

#[test]
fn configured_curated_plugin_ids_from_praxis_home_reads_latest_user_config() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        &tmp.path().join(CONFIG_TOML_FILE),
        r#"[features]
plugins = true

[plugins."slack@openai-curated"]
enabled = true

[plugins."sample@debug"]
enabled = true
"#,
    );

    assert_eq!(
        configured_curated_plugin_ids_from_praxis_home(tmp.path())
            .into_iter()
            .map(|plugin_id| plugin_id.as_key())
            .collect::<Vec<_>>(),
        vec!["slack@openai-curated".to_string()]
    );

    write_file(
        &tmp.path().join(CONFIG_TOML_FILE),
        r#"[features]
plugins = true
"#,
    );

    assert_eq!(
        configured_curated_plugin_ids_from_praxis_home(tmp.path()),
        Vec::<PluginId>::new()
    );
}

#[test]
fn refresh_curated_plugin_cache_returns_false_when_configured_plugins_are_current() {
    let tmp = tempfile::tempdir().unwrap();
    let curated_root = curated_plugins_repo_path(tmp.path());
    write_curated_marketplace(&curated_root, &["slack"]);
    let plugin_id = PluginId::new(
        "slack".to_string(),
        OPENAI_CURATED_MARKETPLACE_NAME.to_string(),
    )
    .unwrap();
    write_plugin(
        &tmp.path().join("plugins/cache/openai-curated"),
        &format!("slack/{TEST_CURATED_PLUGIN_SHA}"),
        "slack",
    );

    assert!(
        !refresh_curated_plugin_cache(tmp.path(), TEST_CURATED_PLUGIN_SHA, &[plugin_id])
            .expect("cache refresh should be a no-op when configured plugins are current")
    );
}

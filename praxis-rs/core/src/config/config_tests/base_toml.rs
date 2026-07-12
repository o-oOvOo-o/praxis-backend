use super::*;

#[test]
fn load_config_normalizes_relative_cwd_override() -> std::io::Result<()> {
    let expected_cwd = AbsolutePathBuf::relative_to_current_dir("nested")?;
    let praxis_home = tempdir()?;
    let config = Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides {
            cwd: Some(PathBuf::from("nested")),
            ..Default::default()
        },
        praxis_home.abs().into_path_buf(),
    )?;

    assert_eq!(config.cwd, expected_cwd);
    Ok(())
}

#[test]
fn explicit_model_provider_override_prevents_model_owner_provider_switch() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
[model_providers.common]
name = "Common"
base_url = "https://token-plan.example.test/compatible-mode/v1"
requires_openai_auth = false
supports_websockets = false
wire_api = "openai_compat"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
requires_openai_auth = false
supports_websockets = false
wire_api = "openai_compat"
"#,
    )
    .expect("TOML deserialization should succeed");
    let praxis_home = tempdir()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            model: Some("deepseek-v4-pro".to_string()),
            model_provider: Some("common".to_string()),
            cwd: Some(praxis_home.path().to_path_buf()),
            ..Default::default()
        },
        praxis_home.path().to_path_buf(),
    )?;

    assert_eq!(config.model, Some("deepseek-v4-pro".to_string()));
    assert_eq!(config.model_provider_id, "common");
    assert!(
        !config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("switched provider")),
        "explicit provider override should not be auto-switched: {:?}",
        config.startup_warnings
    );
    Ok(())
}

#[test]
fn claude_model_selects_built_in_anthropic_provider() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "claude-sonnet-4-6"
"#,
    )
    .expect("TOML deserialization should succeed");
    let praxis_home = tempdir()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(praxis_home.path().to_path_buf()),
            ..Default::default()
        },
        praxis_home.path().to_path_buf(),
    )?;

    assert_eq!(config.model_provider_id, crate::ANTHROPIC_PROVIDER_ID);
    assert!(config.model_provider.is_anthropic());
    assert!(
        config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("switched provider"))
    );
    Ok(())
}

#[test]
fn test_toml_parsing() {
    let history_with_persistence = r#"
[history]
persistence = "save-all"
"#;
    let history_with_persistence_cfg = toml::from_str::<ConfigToml>(history_with_persistence)
        .expect("TOML deserialization should succeed");
    assert_eq!(
        Some(History {
            persistence: HistoryPersistence::SaveAll,
            max_bytes: None,
        }),
        history_with_persistence_cfg.history
    );

    let history_no_persistence = r#"
[history]
persistence = "none"
"#;

    let history_no_persistence_cfg = toml::from_str::<ConfigToml>(history_no_persistence)
        .expect("TOML deserialization should succeed");
    assert_eq!(
        Some(History {
            persistence: HistoryPersistence::None,
            max_bytes: None,
        }),
        history_no_persistence_cfg.history
    );

    let memories = r#"
[memories]
no_memories_if_mcp_or_web_search = true
generate_memories = false
use_memories = false
max_raw_memories_for_consolidation = 512
max_unused_days = 21
max_rollout_age_days = 42
max_rollouts_per_startup = 9
min_rollout_idle_hours = 24
extract_model = "gpt-5-mini"
consolidation_model = "gpt-5"
"#;
    let memories_cfg =
        toml::from_str::<ConfigToml>(memories).expect("TOML deserialization should succeed");
    assert_eq!(
        Some(MemoriesToml {
            no_memories_if_mcp_or_web_search: Some(true),
            generate_memories: Some(false),
            use_memories: Some(false),
            max_raw_memories_for_consolidation: Some(512),
            max_unused_days: Some(21),
            max_rollout_age_days: Some(42),
            max_rollouts_per_startup: Some(9),
            min_rollout_idle_hours: Some(24),
            extract_model: Some("gpt-5-mini".to_string()),
            consolidation_model: Some("gpt-5".to_string()),
        }),
        memories_cfg.memories
    );

    let config = Config::load_from_base_config_with_overrides(
        memories_cfg,
        ConfigOverrides::default(),
        tempdir().expect("tempdir").path().to_path_buf(),
    )
    .expect("load config from memories settings");
    assert_eq!(
        config.memories,
        MemoriesConfig {
            no_memories_if_mcp_or_web_search: true,
            generate_memories: false,
            use_memories: false,
            max_raw_memories_for_consolidation: 512,
            max_unused_days: 21,
            max_rollout_age_days: 42,
            max_rollouts_per_startup: 9,
            min_rollout_idle_hours: 24,
            extract_model: Some("gpt-5-mini".to_string()),
            consolidation_model: Some("gpt-5".to_string()),
        }
    );
}

#[test]
fn parses_bundled_skills_config() {
    let cfg: ConfigToml = toml::from_str(
        r#"
[skills.bundled]
enabled = false
"#,
    )
    .expect("TOML deserialization should succeed");

    assert_eq!(
        cfg.skills,
        Some(SkillsConfig {
            bundled: Some(BundledSkillsConfig { enabled: false }),
            config: Vec::new(),
        })
    );
}

#[test]
fn tools_web_search_true_deserializes_to_none() {
    let cfg: ConfigToml = toml::from_str(
        r#"
[tools]
web_search = true
"#,
    )
    .expect("TOML deserialization should succeed");

    assert_eq!(
        cfg.tools,
        Some(ToolsToml {
            web_search: None,
            view_image: None,
        })
    );
}

#[test]
fn tools_web_search_false_deserializes_to_none() {
    let cfg: ConfigToml = toml::from_str(
        r#"
[tools]
web_search = false
"#,
    )
    .expect("TOML deserialization should succeed");

    assert_eq!(
        cfg.tools,
        Some(ToolsToml {
            web_search: None,
            view_image: None,
        })
    );
}

#[test]
fn rejects_provider_auth_with_env_key() {
    let err = toml::from_str::<ConfigToml>(
        r#"
[model_providers.corp]
name = "Corp"
env_key = "CORP_TOKEN"

[model_providers.corp.auth]
command = "print-token"
"#,
    )
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("model_providers.corp: provider auth cannot be combined with env_key")
    );
}

#[test]
fn rejects_noncanonical_provider_id_that_could_alias_a_credential() {
    let err = toml::from_str::<ConfigToml>(
        r#"
[model_providers." corp "]
name = "Corp"
env_key = "CORP_TOKEN"
"#,
    )
    .unwrap_err();

    assert!(err.to_string().contains("model provider ID ` corp `"));
}

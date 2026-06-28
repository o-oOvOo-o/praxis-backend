use super::*;

#[test]
fn test_requirements_web_search_mode_allowlist_does_not_warn_when_unset() -> anyhow::Result<()> {
    let fixture = create_test_fixture()?;

    let requirements_toml = crate::config_loader::ConfigRequirementsToml {
        allowed_approval_policies: None,
        allowed_sandbox_modes: None,
        allowed_web_search_modes: Some(vec![
            crate::config_loader::WebSearchModeRequirement::Cached,
        ]),
        feature_requirements: None,
        mcp_servers: None,
        apps: None,
        rules: None,
        enforce_residency: None,
        network: None,
        guardian_developer_instructions: None,
    };
    let requirement_source = crate::config_loader::RequirementSource::Unknown;
    let requirement_source_for_error = requirement_source.clone();
    let allowed = vec![WebSearchMode::Disabled, WebSearchMode::Cached];
    let constrained = Constrained::new(WebSearchMode::Cached, move |candidate| {
        if matches!(candidate, WebSearchMode::Cached | WebSearchMode::Disabled) {
            Ok(())
        } else {
            Err(ConstraintError::InvalidValue {
                field_name: "web_search_mode",
                candidate: format!("{candidate:?}"),
                allowed: format!("{allowed:?}"),
                requirement_source: requirement_source_for_error.clone(),
            })
        }
    })?;
    let requirements = crate::config_loader::ConfigRequirements {
        web_search_mode: crate::config_loader::ConstrainedWithSource::new(
            constrained,
            Some(requirement_source),
        ),
        ..Default::default()
    };
    let config_layer_stack =
        crate::config_loader::ConfigLayerStack::new(Vec::new(), requirements, requirements_toml)
            .expect("config layer stack");

    let config = Config::load_config_with_layer_stack(
        fixture.cfg.clone(),
        ConfigOverrides {
            cwd: Some(fixture.cwd_path()),
            ..Default::default()
        },
        fixture.praxis_home(),
        config_layer_stack,
    )?;

    assert!(
        !config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("Configured value for `web_search_mode`")),
        "{:?}",
        config.startup_warnings
    );

    Ok(())
}

#[test]
fn test_set_project_trusted_writes_explicit_tables() -> anyhow::Result<()> {
    let project_dir = Path::new("/some/path");
    let mut doc = DocumentMut::new();

    set_project_trust_level_inner(&mut doc, project_dir, TrustLevel::Trusted)?;

    let contents = doc.to_string();

    let raw_path = project_dir.to_string_lossy();
    let path_str = if raw_path.contains('\\') {
        format!("'{raw_path}'")
    } else {
        format!("\"{raw_path}\"")
    };
    let expected = format!(
        r#"[projects.{path_str}]
trust_level = "trusted"
"#
    );
    assert_eq!(contents, expected);

    Ok(())
}

#[test]
fn test_set_project_trusted_converts_inline_to_explicit() -> anyhow::Result<()> {
    let project_dir = Path::new("/some/path");

    // Seed config.toml with an inline project entry under [projects]
    let raw_path = project_dir.to_string_lossy();
    let path_str = if raw_path.contains('\\') {
        format!("'{raw_path}'")
    } else {
        format!("\"{raw_path}\"")
    };
    // Use a quoted key so backslashes don't require escaping on Windows
    let initial = format!(
        r#"[projects]
{path_str} = {{ trust_level = "untrusted" }}
"#
    );
    let mut doc = initial.parse::<DocumentMut>()?;

    // Run the function; it should convert to explicit tables and set trusted
    set_project_trust_level_inner(&mut doc, project_dir, TrustLevel::Trusted)?;

    let contents = doc.to_string();

    // Assert exact output after conversion to explicit table
    let expected = format!(
        r#"[projects]

[projects.{path_str}]
trust_level = "trusted"
"#
    );
    assert_eq!(contents, expected);

    Ok(())
}

#[test]
fn test_set_project_trusted_migrates_top_level_inline_projects_preserving_entries()
-> anyhow::Result<()> {
    let initial = r#"toplevel = "baz"
projects = { "/Users/mbolin/code/praxis4" = { trust_level = "trusted", foo = "bar" } , "/Users/mbolin/code/praxis3" = { trust_level = "trusted" } }
model = "foo""#;
    let mut doc = initial.parse::<DocumentMut>()?;

    // Approve a new directory
    let new_project = Path::new("/Users/mbolin/code/praxis2");
    set_project_trust_level_inner(&mut doc, new_project, TrustLevel::Trusted)?;

    let contents = doc.to_string();

    // Since we created the [projects] table as part of migration, it is kept implicit.
    // Expect explicit per-project tables, preserving prior entries and appending the new one.
    let expected = r#"toplevel = "baz"
model = "foo"

[projects."/Users/mbolin/code/praxis4"]
trust_level = "trusted"
foo = "bar"

[projects."/Users/mbolin/code/praxis3"]
trust_level = "trusted"

[projects."/Users/mbolin/code/praxis2"]
trust_level = "trusted"
"#;
    assert_eq!(contents, expected);

    Ok(())
}

#[test]
fn test_set_default_oss_provider() -> std::io::Result<()> {
    let temp_dir = TempDir::new()?;
    let praxis_home = temp_dir.path();
    let config_path = praxis_home.join(CONFIG_TOML_FILE);

    // Test setting valid provider on empty config
    set_default_oss_provider(praxis_home, OLLAMA_OSS_PROVIDER_ID)?;
    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("oss_provider = \"ollama\""));

    // Test updating existing config
    std::fs::write(&config_path, "model = \"gpt-4\"\n")?;
    set_default_oss_provider(praxis_home, LMSTUDIO_OSS_PROVIDER_ID)?;
    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("oss_provider = \"lmstudio\""));
    assert!(content.contains("model = \"gpt-4\""));

    // Test overwriting existing oss_provider
    set_default_oss_provider(praxis_home, OLLAMA_OSS_PROVIDER_ID)?;
    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("oss_provider = \"ollama\""));
    assert!(!content.contains("oss_provider = \"lmstudio\""));

    // Test invalid provider
    let result = set_default_oss_provider(praxis_home, "invalid_provider");
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("Invalid OSS provider"));
    assert!(error.to_string().contains("invalid_provider"));

    Ok(())
}

#[test]
fn test_set_default_oss_provider_rejects_legacy_ollama_chat_provider() -> std::io::Result<()> {
    let temp_dir = TempDir::new()?;
    let praxis_home = temp_dir.path();

    let result = set_default_oss_provider(praxis_home, LEGACY_OLLAMA_CHAT_PROVIDER_ID);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        error
            .to_string()
            .contains(OLLAMA_CHAT_PROVIDER_REMOVED_ERROR)
    );

    Ok(())
}

#[test]
fn test_load_config_rejects_legacy_ollama_chat_provider_with_helpful_error() -> std::io::Result<()>
{
    let praxis_home = TempDir::new()?;
    let cfg = ConfigToml {
        model_provider: Some(LEGACY_OLLAMA_CHAT_PROVIDER_ID.to_string()),
        ..Default::default()
    };

    let result = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        praxis_home.path().to_path_buf(),
    );
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(
        error
            .to_string()
            .contains(OLLAMA_CHAT_PROVIDER_REMOVED_ERROR)
    );

    Ok(())
}

#[test]
fn test_untrusted_project_gets_workspace_write_sandbox() -> anyhow::Result<()> {
    let config_with_untrusted = r#"
[projects."/tmp/test"]
trust_level = "untrusted"
"#;

    let cfg = toml::from_str::<ConfigToml>(config_with_untrusted)
        .expect("TOML deserialization should succeed");

    let resolution = cfg.derive_sandbox_policy(
        /*sandbox_mode_override*/ None,
        /*profile_sandbox_mode*/ None,
        WindowsSandboxLevel::Disabled,
        &PathBuf::from("/tmp/test"),
        /*sandbox_policy_constraint*/ None,
    );

    // Verify that untrusted projects get WorkspaceWrite (or ReadOnly on Windows due to downgrade)
    if cfg!(target_os = "windows") {
        assert!(
            matches!(resolution, SandboxPolicy::ReadOnly { .. }),
            "Expected ReadOnly on Windows, got {resolution:?}"
        );
    } else {
        assert!(
            matches!(resolution, SandboxPolicy::WorkspaceWrite { .. }),
            "Expected WorkspaceWrite for untrusted project, got {resolution:?}"
        );
    }

    Ok(())
}

#[test]
fn derive_sandbox_policy_falls_back_to_constraint_value_for_implicit_defaults() -> anyhow::Result<()>
{
    let project_dir = TempDir::new()?;
    let project_path = project_dir.path().to_path_buf();
    let project_key = project_path.to_string_lossy().to_string();
    let cfg = ConfigToml {
        projects: Some(HashMap::from([(
            project_key,
            ProjectConfig {
                trust_level: Some(TrustLevel::Trusted),
            },
        )])),
        ..Default::default()
    };
    let constrained = Constrained::new(SandboxPolicy::DangerFullAccess, |candidate| {
        if matches!(candidate, SandboxPolicy::DangerFullAccess) {
            Ok(())
        } else {
            Err(ConstraintError::InvalidValue {
                field_name: "sandbox_mode",
                candidate: format!("{candidate:?}"),
                allowed: "[DangerFullAccess]".to_string(),
                requirement_source: RequirementSource::Unknown,
            })
        }
    })?;

    let resolution = cfg.derive_sandbox_policy(
        /*sandbox_mode_override*/ None,
        /*profile_sandbox_mode*/ None,
        WindowsSandboxLevel::Disabled,
        &project_path,
        Some(&constrained),
    );

    assert_eq!(resolution, SandboxPolicy::DangerFullAccess);
    Ok(())
}

#[test]
fn derive_sandbox_policy_preserves_windows_downgrade_for_unsupported_fallback() -> anyhow::Result<()>
{
    let project_dir = TempDir::new()?;
    let project_path = project_dir.path().to_path_buf();
    let project_key = project_path.to_string_lossy().to_string();
    let cfg = ConfigToml {
        projects: Some(HashMap::from([(
            project_key,
            ProjectConfig {
                trust_level: Some(TrustLevel::Trusted),
            },
        )])),
        ..Default::default()
    };
    let constrained = Constrained::new(SandboxPolicy::new_workspace_write_policy(), |candidate| {
        if matches!(candidate, SandboxPolicy::WorkspaceWrite { .. }) {
            Ok(())
        } else {
            Err(ConstraintError::InvalidValue {
                field_name: "sandbox_mode",
                candidate: format!("{candidate:?}"),
                allowed: "[WorkspaceWrite]".to_string(),
                requirement_source: RequirementSource::Unknown,
            })
        }
    })?;

    let resolution = cfg.derive_sandbox_policy(
        /*sandbox_mode_override*/ None,
        /*profile_sandbox_mode*/ None,
        WindowsSandboxLevel::Disabled,
        &project_path,
        Some(&constrained),
    );

    if cfg!(target_os = "windows") {
        assert_eq!(resolution, SandboxPolicy::new_read_only_policy());
    } else {
        assert_eq!(resolution, SandboxPolicy::new_workspace_write_policy());
    }
    Ok(())
}

#[test]
fn test_resolve_oss_provider_explicit_override() {
    let config_toml = ConfigToml::default();
    let result = resolve_oss_provider(
        Some("custom-provider"),
        &config_toml,
        /*config_profile*/ None,
    );
    assert_eq!(result, Some("custom-provider".to_string()));
}

#[test]
fn test_resolve_oss_provider_from_profile() {
    let mut profiles = std::collections::HashMap::new();
    let profile = ConfigProfile {
        oss_provider: Some("profile-provider".to_string()),
        ..Default::default()
    };
    profiles.insert("test-profile".to_string(), profile);
    let config_toml = ConfigToml {
        profiles,
        ..Default::default()
    };

    let result = resolve_oss_provider(
        /*explicit_provider*/ None,
        &config_toml,
        Some("test-profile".to_string()),
    );
    assert_eq!(result, Some("profile-provider".to_string()));
}

#[test]
fn test_resolve_oss_provider_from_global_config() {
    let config_toml = ConfigToml {
        oss_provider: Some("global-provider".to_string()),
        ..Default::default()
    };

    let result = resolve_oss_provider(
        /*explicit_provider*/ None,
        &config_toml,
        /*config_profile*/ None,
    );
    assert_eq!(result, Some("global-provider".to_string()));
}

#[test]
fn test_resolve_oss_provider_profile_fallback_to_global() {
    let mut profiles = std::collections::HashMap::new();
    let profile = ConfigProfile::default(); // No oss_provider set
    profiles.insert("test-profile".to_string(), profile);
    let config_toml = ConfigToml {
        oss_provider: Some("global-provider".to_string()),
        profiles,
        ..Default::default()
    };

    let result = resolve_oss_provider(
        /*explicit_provider*/ None,
        &config_toml,
        Some("test-profile".to_string()),
    );
    assert_eq!(result, Some("global-provider".to_string()));
}

#[test]
fn test_resolve_oss_provider_none_when_not_configured() {
    let config_toml = ConfigToml::default();
    let result = resolve_oss_provider(
        /*explicit_provider*/ None,
        &config_toml,
        /*config_profile*/ None,
    );
    assert_eq!(result, None);
}

#[test]
fn test_resolve_oss_provider_explicit_overrides_all() {
    let mut profiles = std::collections::HashMap::new();
    let profile = ConfigProfile {
        oss_provider: Some("profile-provider".to_string()),
        ..Default::default()
    };
    profiles.insert("test-profile".to_string(), profile);
    let config_toml = ConfigToml {
        oss_provider: Some("global-provider".to_string()),
        profiles,
        ..Default::default()
    };

    let result = resolve_oss_provider(
        Some("explicit-provider"),
        &config_toml,
        Some("test-profile".to_string()),
    );
    assert_eq!(result, Some("explicit-provider".to_string()));
}

#[test]
fn config_toml_deserializes_mcp_oauth_callback_port() {
    let toml = r#"mcp_oauth_callback_port = 4321"#;
    let cfg: ConfigToml =
        toml::from_str(toml).expect("TOML deserialization should succeed for callback port");
    assert_eq!(cfg.mcp_oauth_callback_port, Some(4321));
}

#[test]
fn config_toml_deserializes_mcp_oauth_callback_url() {
    let toml = r#"mcp_oauth_callback_url = "https://example.com/callback""#;
    let cfg: ConfigToml =
        toml::from_str(toml).expect("TOML deserialization should succeed for callback URL");
    assert_eq!(
        cfg.mcp_oauth_callback_url.as_deref(),
        Some("https://example.com/callback")
    );
}

#[test]
fn config_loads_mcp_oauth_callback_port_from_toml() -> std::io::Result<()> {
    let praxis_home = TempDir::new()?;
    let toml = r#"
model = "gpt-5.1"
mcp_oauth_callback_port = 5678
"#;
    let cfg: ConfigToml =
        toml::from_str(toml).expect("TOML deserialization should succeed for callback port");

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        praxis_home.path().to_path_buf(),
    )?;

    assert_eq!(config.mcp_oauth_callback_port, Some(5678));
    Ok(())
}

#[test]
fn config_loads_allow_login_shell_from_toml() -> std::io::Result<()> {
    let praxis_home = TempDir::new()?;
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "gpt-5.1"
allow_login_shell = false
"#,
    )
    .expect("TOML deserialization should succeed for allow_login_shell");

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        praxis_home.path().to_path_buf(),
    )?;

    assert!(!config.permissions.allow_login_shell);
    Ok(())
}

#[test]
fn config_loads_mcp_oauth_callback_url_from_toml() -> std::io::Result<()> {
    let praxis_home = TempDir::new()?;
    let toml = r#"
model = "gpt-5.1"
mcp_oauth_callback_url = "https://example.com/callback"
"#;
    let cfg: ConfigToml =
        toml::from_str(toml).expect("TOML deserialization should succeed for callback URL");

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        praxis_home.path().to_path_buf(),
    )?;

    assert_eq!(
        config.mcp_oauth_callback_url.as_deref(),
        Some("https://example.com/callback")
    );
    Ok(())
}

#[test]
fn test_untrusted_project_gets_unless_trusted_approval_policy() -> anyhow::Result<()> {
    let praxis_home = TempDir::new()?;
    let test_project_dir = TempDir::new()?;
    let test_path = test_project_dir.path();

    let config = Config::load_from_base_config_with_overrides(
        ConfigToml {
            projects: Some(HashMap::from([(
                test_path.to_string_lossy().to_string(),
                ProjectConfig {
                    trust_level: Some(TrustLevel::Untrusted),
                },
            )])),
            ..Default::default()
        },
        ConfigOverrides {
            cwd: Some(test_path.to_path_buf()),
            ..Default::default()
        },
        praxis_home.path().to_path_buf(),
    )?;

    // Verify that untrusted projects get UnlessTrusted approval policy
    assert_eq!(
        config.permissions.approval_policy.value(),
        AskForApproval::UnlessTrusted,
        "Expected UnlessTrusted approval policy for untrusted project"
    );

    // Verify that untrusted projects still get WorkspaceWrite sandbox (or ReadOnly on Windows)
    if cfg!(target_os = "windows") {
        assert!(
            matches!(
                config.permissions.sandbox_policy.get(),
                SandboxPolicy::ReadOnly { .. }
            ),
            "Expected ReadOnly on Windows"
        );
    } else {
        assert!(
            matches!(
                config.permissions.sandbox_policy.get(),
                SandboxPolicy::WorkspaceWrite { .. }
            ),
            "Expected WorkspaceWrite sandbox for untrusted project"
        );
    }

    Ok(())
}

use super::*;

#[cfg(target_os = "macos")]
#[tokio::test]
async fn managed_preferences_take_highest_precedence() {
    use base64::Engine;

    let tmp = tempdir().expect("tempdir");
    let managed_path = tmp.path().join("managed_config.toml");

    std::fs::write(
        tmp.path().join(CONFIG_TOML_FILE),
        r#"[nested]
value = "base"
"#,
    )
    .expect("write base");
    std::fs::write(
        &managed_path,
        r#"[nested]
value = "managed_config"
flag = true
"#,
    )
    .expect("write managed config");
    let raw_managed_preferences = r#"
# managed profile
[nested]
value = "managed"
flag = false
"#;

    let overrides = LoaderOverrides {
        managed_config_path: Some(managed_path),
        managed_preferences_base64: Some(
            base64::prelude::BASE64_STANDARD.encode(raw_managed_preferences.as_bytes()),
        ),
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
    let nested = loaded
        .get("nested")
        .and_then(|v| v.as_table())
        .expect("nested table");
    assert_eq!(
        nested.get("value"),
        Some(&TomlValue::String("managed".to_string()))
    );
    assert_eq!(nested.get("flag"), Some(&TomlValue::Boolean(false)));
    let mdm_layer = state
        .layers_high_to_low()
        .into_iter()
        .find(|layer| {
            matches!(
                layer.name,
                super::ConfigLayerSource::LegacyManagedConfigTomlFromMdm
            )
        })
        .expect("mdm layer");
    let raw = mdm_layer.raw_toml().expect("preserved mdm toml");
    assert!(raw.contains("# managed profile"));
    assert!(raw.contains("value = \"managed\""));
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn managed_preferences_expand_home_directory_in_workspace_write_roots() -> anyhow::Result<()>
{
    use base64::Engine;

    let Some(home) = dirs::home_dir() else {
        return Ok(());
    };
    let tmp = tempdir()?;

    let config = ConfigBuilder::default()
        .praxis_home(tmp.path().to_path_buf())
        .fallback_cwd(Some(tmp.path().to_path_buf()))
        .loader_overrides(LoaderOverrides {
            managed_config_path: Some(tmp.path().join("managed_config.toml")),
            managed_preferences_base64: Some(
                base64::prelude::BASE64_STANDARD.encode(
                    r#"
sandbox_mode = "workspace-write"
[sandbox_workspace_write]
writable_roots = ["~/code"]
"#
                    .as_bytes(),
                ),
            ),
            macos_managed_config_requirements_base64: None,
        })
        .build()
        .await?;

    let expected_root = AbsolutePathBuf::from_absolute_path(home.join("code"))?;
    match config.permissions.sandbox_policy.get() {
        SandboxPolicy::WorkspaceWrite { writable_roots, .. } => {
            assert_eq!(
                writable_roots
                    .iter()
                    .filter(|root| **root == expected_root)
                    .count(),
                1,
            );
        }
        other => panic!("expected workspace-write policy, got {other:?}"),
    }

    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn managed_preferences_requirements_are_applied() -> anyhow::Result<()> {
    use base64::Engine;

    let tmp = tempdir()?;

    let state = load_config_layers_state(
        tmp.path(),
        Some(AbsolutePathBuf::try_from(tmp.path())?),
        &[] as &[(String, TomlValue)],
        LoaderOverrides {
            managed_config_path: Some(tmp.path().join("managed_config.toml")),
            managed_preferences_base64: Some(String::new()),
            macos_managed_config_requirements_base64: Some(
                base64::prelude::BASE64_STANDARD.encode(
                    r#"
allowed_approval_policies = ["never"]
allowed_sandbox_modes = ["read-only"]
"#
                    .as_bytes(),
                ),
            ),
        },
        CloudRequirementsLoader::default(),
    )
    .await?;

    assert_eq!(
        state.requirements().approval_policy.value(),
        AskForApproval::Never
    );
    assert_eq!(
        *state.requirements().sandbox_policy.get(),
        SandboxPolicy::new_read_only_policy()
    );
    assert!(
        state
            .requirements()
            .approval_policy
            .can_set(&AskForApproval::OnRequest)
            .is_err()
    );
    assert!(
        state
            .requirements()
            .sandbox_policy
            .can_set(&SandboxPolicy::WorkspaceWrite {
                writable_roots: Vec::new(),
                read_only_access: Default::default(),
                network_access: false,
                exclude_tmpdir_env_var: false,
                exclude_slash_tmp: false,
            })
            .is_err()
    );

    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn managed_preferences_requirements_take_precedence() -> anyhow::Result<()> {
    use base64::Engine;

    let tmp = tempdir()?;
    let managed_path = tmp.path().join("managed_config.toml");

    tokio::fs::write(&managed_path, "approval_policy = \"on-request\"\n").await?;

    let state = load_config_layers_state(
        tmp.path(),
        Some(AbsolutePathBuf::try_from(tmp.path())?),
        &[] as &[(String, TomlValue)],
        LoaderOverrides {
            managed_config_path: Some(managed_path),
            managed_preferences_base64: Some(String::new()),
            macos_managed_config_requirements_base64: Some(
                base64::prelude::BASE64_STANDARD.encode(
                    r#"
allowed_approval_policies = ["never"]
"#
                    .as_bytes(),
                ),
            ),
        },
        CloudRequirementsLoader::default(),
    )
    .await?;

    assert_eq!(
        state.requirements().approval_policy.value(),
        AskForApproval::Never
    );
    assert!(
        state
            .requirements()
            .approval_policy
            .can_set(&AskForApproval::OnRequest)
            .is_err()
    );

    Ok(())
}

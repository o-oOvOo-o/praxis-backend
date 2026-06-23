use super::*;

#[tokio::test(flavor = "current_thread")]
async fn load_requirements_toml_produces_expected_constraints() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let requirements_file = tmp.path().join("requirements.toml");
    tokio::fs::write(
        &requirements_file,
        r#"
allowed_approval_policies = ["never", "on-request"]
allowed_web_search_modes = ["cached"]
enforce_residency = "us"

[features]
personality = true
"#,
    )
    .await?;

    let mut config_requirements_toml = ConfigRequirementsWithSources::default();
    load_requirements_toml(&mut config_requirements_toml, &requirements_file).await?;

    assert_eq!(
        config_requirements_toml
            .allowed_approval_policies
            .as_deref()
            .cloned(),
        Some(vec![AskForApproval::Never, AskForApproval::OnRequest])
    );
    assert_eq!(
        config_requirements_toml
            .allowed_web_search_modes
            .as_deref()
            .cloned(),
        Some(vec![crate::config_loader::WebSearchModeRequirement::Cached])
    );
    assert_eq!(
        config_requirements_toml
            .feature_requirements
            .as_ref()
            .map(|requirements| requirements.value.clone()),
        Some(crate::config_loader::FeatureRequirementsToml {
            entries: BTreeMap::from([("personality".to_string(), true)]),
        })
    );
    let config_requirements: ConfigRequirements = config_requirements_toml.try_into()?;
    assert_eq!(
        config_requirements.approval_policy.value(),
        AskForApproval::Never
    );
    config_requirements
        .approval_policy
        .can_set(&AskForApproval::Never)?;
    assert!(
        config_requirements
            .approval_policy
            .can_set(&AskForApproval::OnFailure)
            .is_err()
    );
    assert_eq!(
        config_requirements.web_search_mode.value(),
        WebSearchMode::Cached
    );
    config_requirements
        .web_search_mode
        .can_set(&WebSearchMode::Cached)?;
    config_requirements
        .web_search_mode
        .can_set(&WebSearchMode::Cached)?;
    config_requirements
        .web_search_mode
        .can_set(&WebSearchMode::Disabled)?;
    assert!(
        config_requirements
            .web_search_mode
            .can_set(&WebSearchMode::Live)
            .is_err()
    );
    assert_eq!(
        config_requirements.enforce_residency.value(),
        Some(crate::config_loader::ResidencyRequirement::Us)
    );
    assert_eq!(
        config_requirements
            .feature_requirements
            .as_ref()
            .map(|requirements| requirements.value.clone()),
        Some(crate::config_loader::FeatureRequirementsToml {
            entries: BTreeMap::from([("personality".to_string(), true)]),
        })
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn cloud_requirements_take_precedence_over_mdm_requirements() -> anyhow::Result<()> {
    use base64::Engine;

    let tmp = tempdir()?;
    let state = load_config_layers_state(
        tmp.path(),
        Some(AbsolutePathBuf::try_from(tmp.path())?),
        &[] as &[(String, TomlValue)],
        LoaderOverrides {
            macos_managed_config_requirements_base64: Some(
                base64::prelude::BASE64_STANDARD.encode(
                    r#"
allowed_approval_policies = ["on-request"]
"#
                    .as_bytes(),
                ),
            ),
            ..LoaderOverrides::default()
        },
        CloudRequirementsLoader::new(async {
            Ok(Some(ConfigRequirementsToml {
                allowed_approval_policies: Some(vec![AskForApproval::Never]),
                allowed_sandbox_modes: None,
                allowed_web_search_modes: None,
                feature_requirements: None,
                mcp_servers: None,
                apps: None,
                rules: None,
                enforce_residency: None,
                network: None,
                guardian_developer_instructions: None,
            }))
        }),
    )
    .await?;

    assert_eq!(
        state.requirements().approval_policy.value(),
        AskForApproval::Never
    );
    assert_eq!(
        state
            .requirements()
            .approval_policy
            .can_set(&AskForApproval::OnRequest),
        Err(ConstraintError::InvalidValue {
            field_name: "approval_policy",
            candidate: "OnRequest".into(),
            allowed: "[Never]".into(),
            requirement_source: RequirementSource::CloudRequirements,
        })
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn cloud_requirements_are_not_overwritten_by_system_requirements() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let requirements_file = tmp.path().join("requirements.toml");
    tokio::fs::write(
        &requirements_file,
        r#"
allowed_approval_policies = ["on-request"]
"#,
    )
    .await?;

    let mut config_requirements_toml = ConfigRequirementsWithSources::default();
    config_requirements_toml.merge_unset_fields(
        RequirementSource::CloudRequirements,
        ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
            guardian_developer_instructions: None,
        },
    );
    load_requirements_toml(&mut config_requirements_toml, &requirements_file).await?;

    assert_eq!(
        config_requirements_toml
            .allowed_approval_policies
            .as_ref()
            .map(|sourced| sourced.value.clone()),
        Some(vec![AskForApproval::Never])
    );
    assert_eq!(
        config_requirements_toml
            .allowed_approval_policies
            .as_ref()
            .map(|sourced| sourced.source.clone()),
        Some(RequirementSource::CloudRequirements)
    );

    Ok(())
}

#[tokio::test]
async fn load_config_layers_includes_cloud_requirements() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&praxis_home).await?;
    let cwd = AbsolutePathBuf::from_absolute_path(tmp.path())?;

    let requirements = ConfigRequirementsToml {
        allowed_approval_policies: Some(vec![AskForApproval::Never]),
        allowed_sandbox_modes: None,
        allowed_web_search_modes: None,
        feature_requirements: None,
        mcp_servers: None,
        apps: None,
        rules: None,
        enforce_residency: None,
        network: None,
        guardian_developer_instructions: None,
    };
    let expected = requirements.clone();
    let cloud_requirements = CloudRequirementsLoader::new(async move { Ok(Some(requirements)) });

    let layers = load_config_layers_state(
        &praxis_home,
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        cloud_requirements,
    )
    .await?;

    assert_eq!(
        layers.requirements_toml().allowed_approval_policies,
        expected.allowed_approval_policies
    );
    assert_eq!(
        layers
            .requirements()
            .approval_policy
            .can_set(&AskForApproval::OnRequest),
        Err(ConstraintError::InvalidValue {
            field_name: "approval_policy",
            candidate: "OnRequest".into(),
            allowed: "[Never]".into(),
            requirement_source: RequirementSource::CloudRequirements,
        })
    );

    Ok(())
}

#[tokio::test]
async fn load_config_layers_fails_when_cloud_requirements_loader_fails() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let praxis_home = tmp.path().join("home");
    tokio::fs::create_dir_all(&praxis_home).await?;
    let cwd = AbsolutePathBuf::from_absolute_path(tmp.path())?;

    let err = load_config_layers_state(
        &praxis_home,
        Some(cwd),
        &[] as &[(String, TomlValue)],
        LoaderOverrides::default(),
        CloudRequirementsLoader::new(async {
            Err(CloudRequirementsLoadError::new(
                praxis_config::CloudRequirementsLoadErrorCode::RequestFailed,
                /*status_code*/ None,
                "cloud requirements failed",
            ))
        }),
    )
    .await
    .expect_err("cloud requirements failure should fail closed");

    assert_eq!(err.kind(), std::io::ErrorKind::Other);
    assert!(err.to_string().contains("cloud requirements failed"));

    Ok(())
}

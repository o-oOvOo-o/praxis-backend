use super::*;
use anyhow::Result;
use praxis_execpolicy::Decision;
use praxis_execpolicy::Evaluation;
use praxis_execpolicy::RuleMatch;
use praxis_protocol::protocol::NetworkAccess;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use toml::from_str;

fn tokens(cmd: &[&str]) -> Vec<String> {
    cmd.iter().map(std::string::ToString::to_string).collect()
}

fn system_requirements_toml_file_for_test() -> Result<AbsolutePathBuf> {
    Ok(AbsolutePathBuf::try_from(
        std::env::temp_dir().join("requirements.toml"),
    )?)
}

fn with_unknown_source(toml: ConfigRequirementsToml) -> ConfigRequirementsWithSources {
    let ConfigRequirementsToml {
        allowed_approval_policies,
        allowed_sandbox_modes,
        allowed_web_search_modes,
        feature_requirements,
        mcp_servers,
        apps,
        rules,
        enforce_residency,
        network,
        guardian_developer_instructions,
    } = toml;
    ConfigRequirementsWithSources {
        allowed_approval_policies: allowed_approval_policies
            .map(|value| Sourced::new(value, RequirementSource::Unknown)),
        allowed_sandbox_modes: allowed_sandbox_modes
            .map(|value| Sourced::new(value, RequirementSource::Unknown)),
        allowed_web_search_modes: allowed_web_search_modes
            .map(|value| Sourced::new(value, RequirementSource::Unknown)),
        feature_requirements: feature_requirements
            .map(|value| Sourced::new(value, RequirementSource::Unknown)),
        mcp_servers: mcp_servers.map(|value| Sourced::new(value, RequirementSource::Unknown)),
        apps: apps.map(|value| Sourced::new(value, RequirementSource::Unknown)),
        rules: rules.map(|value| Sourced::new(value, RequirementSource::Unknown)),
        enforce_residency: enforce_residency
            .map(|value| Sourced::new(value, RequirementSource::Unknown)),
        network: network.map(|value| Sourced::new(value, RequirementSource::Unknown)),
        guardian_developer_instructions: guardian_developer_instructions
            .map(|value| Sourced::new(value, RequirementSource::Unknown)),
    }
}

#[test]
fn merge_unset_fields_copies_every_field_and_sets_sources() {
    let mut target = ConfigRequirementsWithSources::default();
    let source = RequirementSource::MdmManagedPreferences {
        domain: "com.openai.praxis".to_string(),
        key: "requirements".to_string(),
    };

    let allowed_approval_policies = vec![AskForApproval::UnlessTrusted, AskForApproval::Never];
    let allowed_sandbox_modes = vec![
        SandboxModeRequirement::WorkspaceWrite,
        SandboxModeRequirement::DangerFullAccess,
    ];
    let allowed_web_search_modes = vec![
        WebSearchModeRequirement::Cached,
        WebSearchModeRequirement::Live,
    ];
    let feature_requirements = FeatureRequirementsToml {
        entries: BTreeMap::from([("personality".to_string(), true)]),
    };
    let enforce_residency = ResidencyRequirement::Us;
    let enforce_source = source.clone();
    let guardian_developer_instructions = "Use the company-managed guardian policy.".to_string();

    // Intentionally constructed without `..Default::default()` so adding a new field to
    // `ConfigRequirementsToml` forces this test to be updated.
    let other = ConfigRequirementsToml {
        allowed_approval_policies: Some(allowed_approval_policies.clone()),
        allowed_sandbox_modes: Some(allowed_sandbox_modes.clone()),
        allowed_web_search_modes: Some(allowed_web_search_modes.clone()),
        feature_requirements: Some(feature_requirements.clone()),
        mcp_servers: None,
        apps: None,
        rules: None,
        enforce_residency: Some(enforce_residency),
        network: None,
        guardian_developer_instructions: Some(guardian_developer_instructions.clone()),
    };

    target.merge_unset_fields(source.clone(), other);

    assert_eq!(
        target,
        ConfigRequirementsWithSources {
            allowed_approval_policies: Some(Sourced::new(
                allowed_approval_policies,
                source.clone()
            )),
            allowed_sandbox_modes: Some(Sourced::new(allowed_sandbox_modes, source.clone(),)),
            allowed_web_search_modes: Some(Sourced::new(
                allowed_web_search_modes,
                enforce_source.clone(),
            )),
            feature_requirements: Some(Sourced::new(feature_requirements, enforce_source.clone(),)),
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: Some(Sourced::new(enforce_residency, enforce_source)),
            network: None,
            guardian_developer_instructions: Some(Sourced::new(
                guardian_developer_instructions,
                source,
            )),
        }
    );
}

#[test]
fn merge_unset_fields_fills_missing_values() -> Result<()> {
    let source: ConfigRequirementsToml = from_str(
        r#"
            allowed_approval_policies = ["on-request"]
        "#,
    )?;

    let source_location = RequirementSource::MdmManagedPreferences {
        domain: "com.openai.praxis".to_string(),
        key: "allowed_approval_policies".to_string(),
    };

    let mut empty_target = ConfigRequirementsWithSources::default();
    empty_target.merge_unset_fields(source_location.clone(), source);
    assert_eq!(
        empty_target,
        ConfigRequirementsWithSources {
            allowed_approval_policies: Some(Sourced::new(
                vec![AskForApproval::OnRequest],
                source_location,
            )),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
            guardian_developer_instructions: None,
        }
    );
    Ok(())
}

#[test]
fn merge_unset_fields_does_not_overwrite_existing_values() -> Result<()> {
    let existing_source = RequirementSource::MdmManagedPreferences {
        domain: "com.openai.praxis".to_string(),
        key: "allowed_approval_policies".to_string(),
    };
    let mut populated_target = ConfigRequirementsWithSources::default();
    let populated_requirements: ConfigRequirementsToml = from_str(
        r#"
            allowed_approval_policies = ["never"]
        "#,
    )?;
    populated_target.merge_unset_fields(existing_source.clone(), populated_requirements);

    let source: ConfigRequirementsToml = from_str(
        r#"
            allowed_approval_policies = ["on-request"]
        "#,
    )?;
    let source_location = RequirementSource::MdmManagedPreferences {
        domain: "com.openai.praxis".to_string(),
        key: "allowed_approval_policies".to_string(),
    };
    populated_target.merge_unset_fields(source_location, source);

    assert_eq!(
        populated_target,
        ConfigRequirementsWithSources {
            allowed_approval_policies: Some(Sourced::new(
                vec![AskForApproval::Never],
                existing_source,
            )),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
            guardian_developer_instructions: None,
        }
    );
    Ok(())
}

#[test]
fn merge_unset_fields_ignores_blank_guardian_override() {
    let mut target = ConfigRequirementsWithSources::default();
    target.merge_unset_fields(
        RequirementSource::CloudRequirements,
        ConfigRequirementsToml {
            guardian_developer_instructions: Some("   \n\t".to_string()),
            ..Default::default()
        },
    );
    target.merge_unset_fields(
        RequirementSource::SystemRequirementsToml {
            file: system_requirements_toml_file_for_test().expect("system requirements.toml path"),
        },
        ConfigRequirementsToml {
            guardian_developer_instructions: Some("Use the system guardian policy.".to_string()),
            ..Default::default()
        },
    );

    assert_eq!(
        target.guardian_developer_instructions,
        Some(Sourced::new(
            "Use the system guardian policy.".to_string(),
            RequirementSource::SystemRequirementsToml {
                file: system_requirements_toml_file_for_test()
                    .expect("system requirements.toml path"),
            },
        )),
    );
}

#[test]
fn deserialize_guardian_developer_instructions() -> Result<()> {
    let requirements: ConfigRequirementsToml = from_str(
        r#"
guardian_developer_instructions = """
Use the cloud-managed guardian policy.
"""
"#,
    )?;

    assert_eq!(
        requirements.guardian_developer_instructions.as_deref(),
        Some("Use the cloud-managed guardian policy.\n")
    );
    Ok(())
}

#[test]
fn blank_guardian_developer_instructions_is_empty() -> Result<()> {
    let requirements: ConfigRequirementsToml = from_str(
        r#"
guardian_developer_instructions = """

"""
"#,
    )?;

    assert!(requirements.is_empty());
    Ok(())
}

#[test]
fn deserialize_apps_requirements() -> Result<()> {
    let toml_str = r#"
        [apps.connector_123123]
        enabled = false
    "#;
    let requirements: ConfigRequirementsToml = from_str(toml_str)?;

    assert_eq!(
        requirements.apps,
        Some(AppsRequirementsToml {
            apps: BTreeMap::from([(
                "connector_123123".to_string(),
                AppRequirementToml {
                    enabled: Some(false),
                },
            )]),
        })
    );
    Ok(())
}

fn apps_requirements(entries: &[(&str, Option<bool>)]) -> AppsRequirementsToml {
    AppsRequirementsToml {
        apps: entries
            .iter()
            .map(|(app_id, enabled)| {
                (
                    (*app_id).to_string(),
                    AppRequirementToml { enabled: *enabled },
                )
            })
            .collect(),
    }
}

#[test]
fn merge_enablement_settings_descending_unions_distinct_apps() {
    let mut merged = apps_requirements(&[("connector_high", Some(false))]);
    let lower = apps_requirements(&[("connector_low", Some(true))]);

    merge_enablement_settings_descending(&mut merged, lower);

    assert_eq!(
        merged,
        apps_requirements(&[
            ("connector_high", Some(false)),
            ("connector_low", Some(true))
        ]),
    );
}

#[test]
fn merge_enablement_settings_descending_prefers_false_from_lower_precedence() {
    let mut merged = apps_requirements(&[("connector_123123", Some(true))]);
    let lower = apps_requirements(&[("connector_123123", Some(false))]);

    merge_enablement_settings_descending(&mut merged, lower);

    assert_eq!(
        merged,
        apps_requirements(&[("connector_123123", Some(false))]),
    );
}

#[test]
fn merge_enablement_settings_descending_keeps_higher_true_when_lower_is_unset() {
    let mut merged = apps_requirements(&[("connector_123123", Some(true))]);
    let lower = apps_requirements(&[("connector_123123", None)]);

    merge_enablement_settings_descending(&mut merged, lower);

    assert_eq!(
        merged,
        apps_requirements(&[("connector_123123", Some(true))]),
    );
}

#[test]
fn merge_enablement_settings_descending_uses_lower_value_when_higher_missing() {
    let mut merged = apps_requirements(&[]);
    let lower = apps_requirements(&[("connector_123123", Some(true))]);

    merge_enablement_settings_descending(&mut merged, lower);

    assert_eq!(
        merged,
        apps_requirements(&[("connector_123123", Some(true))]),
    );
}

#[test]
fn merge_enablement_settings_descending_preserves_higher_false_when_lower_missing_app() {
    let mut merged = apps_requirements(&[("connector_123123", Some(false))]);
    let lower = apps_requirements(&[]);

    merge_enablement_settings_descending(&mut merged, lower);

    assert_eq!(
        merged,
        apps_requirements(&[("connector_123123", Some(false))]),
    );
}

#[test]
fn merge_unset_fields_merges_apps_across_sources_with_enabled_evaluation() {
    let higher_source = RequirementSource::CloudRequirements;
    let lower_source = RequirementSource::MdmManagedPreferences {
        domain: "com.openai.praxis".to_string(),
        key: "apps".to_string(),
    };
    let mut target = ConfigRequirementsWithSources::default();

    target.merge_unset_fields(
        higher_source.clone(),
        ConfigRequirementsToml {
            apps: Some(apps_requirements(&[
                ("connector_high", Some(true)),
                ("connector_shared", Some(true)),
            ])),
            ..Default::default()
        },
    );
    target.merge_unset_fields(
        lower_source,
        ConfigRequirementsToml {
            apps: Some(apps_requirements(&[
                ("connector_low", Some(false)),
                ("connector_shared", Some(false)),
            ])),
            ..Default::default()
        },
    );

    let apps = target.apps.expect("apps should be present");
    assert_eq!(
        apps.value,
        apps_requirements(&[
            ("connector_high", Some(true)),
            ("connector_low", Some(false)),
            ("connector_shared", Some(false)),
        ])
    );
    assert_eq!(apps.source, higher_source);
}

#[test]
fn merge_unset_fields_apps_empty_higher_source_does_not_block_lower_disables() {
    let mut target = ConfigRequirementsWithSources::default();

    target.merge_unset_fields(
        RequirementSource::CloudRequirements,
        ConfigRequirementsToml {
            apps: Some(apps_requirements(&[])),
            ..Default::default()
        },
    );
    target.merge_unset_fields(
        RequirementSource::MdmManagedPreferences {
            domain: "com.openai.praxis".to_string(),
            key: "apps".to_string(),
        },
        ConfigRequirementsToml {
            apps: Some(apps_requirements(&[("connector_123123", Some(false))])),
            ..Default::default()
        },
    );

    assert_eq!(
        target.apps.map(|apps| apps.value),
        Some(apps_requirements(&[("connector_123123", Some(false))])),
    );
}

#[test]
fn constraint_error_includes_requirement_source() -> Result<()> {
    let source: ConfigRequirementsToml = from_str(
        r#"
            allowed_approval_policies = ["on-request"]
            allowed_sandbox_modes = ["read-only"]
        "#,
    )?;

    let requirements_toml_file = system_requirements_toml_file_for_test()?;
    let source_location = RequirementSource::SystemRequirementsToml {
        file: requirements_toml_file,
    };

    let mut target = ConfigRequirementsWithSources::default();
    target.merge_unset_fields(source_location.clone(), source);
    let requirements = ConfigRequirements::try_from(target)?;

    assert_eq!(
        requirements.approval_policy.can_set(&AskForApproval::Never),
        Err(ConstraintError::InvalidValue {
            field_name: "approval_policy",
            candidate: "Never".into(),
            allowed: "[OnRequest]".into(),
            requirement_source: source_location.clone(),
        })
    );
    assert_eq!(
        requirements
            .sandbox_policy
            .can_set(&SandboxPolicy::DangerFullAccess),
        Err(ConstraintError::InvalidValue {
            field_name: "sandbox_mode",
            candidate: "DangerFullAccess".into(),
            allowed: "[ReadOnly]".into(),
            requirement_source: source_location,
        })
    );

    Ok(())
}

#[test]
fn constraint_error_includes_cloud_requirements_source() -> Result<()> {
    let source: ConfigRequirementsToml = from_str(
        r#"
            allowed_approval_policies = ["on-request"]
        "#,
    )?;

    let source_location = RequirementSource::CloudRequirements;

    let mut target = ConfigRequirementsWithSources::default();
    target.merge_unset_fields(source_location.clone(), source);
    let requirements = ConfigRequirements::try_from(target)?;

    assert_eq!(
        requirements.approval_policy.can_set(&AskForApproval::Never),
        Err(ConstraintError::InvalidValue {
            field_name: "approval_policy",
            candidate: "Never".into(),
            allowed: "[OnRequest]".into(),
            requirement_source: source_location,
        })
    );

    Ok(())
}

#[test]
fn constrained_fields_store_requirement_source() -> Result<()> {
    let source: ConfigRequirementsToml = from_str(
        r#"
            allowed_approval_policies = ["on-request"]
            allowed_sandbox_modes = ["read-only"]
            allowed_web_search_modes = ["cached"]
            enforce_residency = "us"
            [features]
            personality = true
        "#,
    )?;

    let source_location = RequirementSource::CloudRequirements;
    let mut target = ConfigRequirementsWithSources::default();
    target.merge_unset_fields(source_location.clone(), source);
    let requirements = ConfigRequirements::try_from(target)?;

    assert_eq!(
        requirements.approval_policy.source,
        Some(source_location.clone())
    );
    assert_eq!(
        requirements.sandbox_policy.source,
        Some(source_location.clone())
    );
    assert_eq!(
        requirements.web_search_mode.source,
        Some(source_location.clone())
    );
    assert_eq!(
        requirements
            .feature_requirements
            .as_ref()
            .map(|requirements| requirements.source.clone()),
        Some(source_location.clone())
    );
    assert_eq!(requirements.enforce_residency.source, Some(source_location));

    Ok(())
}

#[test]
fn deserialize_allowed_approval_policies() -> Result<()> {
    let toml_str = r#"
        allowed_approval_policies = ["untrusted", "on-request"]
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements: ConfigRequirements = with_unknown_source(config).try_into()?;

    assert_eq!(
        requirements.approval_policy.value(),
        AskForApproval::UnlessTrusted,
        "currently, there is no way to specify the default value for approval policy in the toml, so it picks the first allowed value"
    );
    assert!(
        requirements
            .approval_policy
            .can_set(&AskForApproval::UnlessTrusted)
            .is_ok()
    );
    assert_eq!(
        requirements
            .approval_policy
            .can_set(&AskForApproval::OnFailure),
        Err(ConstraintError::InvalidValue {
            field_name: "approval_policy",
            candidate: "OnFailure".into(),
            allowed: "[UnlessTrusted, OnRequest]".into(),
            requirement_source: RequirementSource::Unknown,
        })
    );
    assert!(
        requirements
            .approval_policy
            .can_set(&AskForApproval::OnRequest)
            .is_ok()
    );
    assert_eq!(
        requirements.approval_policy.can_set(&AskForApproval::Never),
        Err(ConstraintError::InvalidValue {
            field_name: "approval_policy",
            candidate: "Never".into(),
            allowed: "[UnlessTrusted, OnRequest]".into(),
            requirement_source: RequirementSource::Unknown,
        })
    );
    assert!(
        requirements
            .sandbox_policy
            .can_set(&SandboxPolicy::new_read_only_policy())
            .is_ok()
    );

    Ok(())
}

#[test]
fn deserialize_allowed_sandbox_modes() -> Result<()> {
    let toml_str = r#"
        allowed_sandbox_modes = ["read-only", "workspace-write"]
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements: ConfigRequirements = with_unknown_source(config).try_into()?;

    let root = if cfg!(windows) { "C:\\repo" } else { "/repo" };
    assert!(
        requirements
            .sandbox_policy
            .can_set(&SandboxPolicy::new_read_only_policy())
            .is_ok()
    );
    assert!(
        requirements
            .sandbox_policy
            .can_set(&SandboxPolicy::WorkspaceWrite {
                writable_roots: vec![AbsolutePathBuf::from_absolute_path(root)?],
                read_only_access: Default::default(),
                network_access: false,
                exclude_tmpdir_env_var: false,
                exclude_slash_tmp: false,
            })
            .is_ok()
    );
    assert_eq!(
        requirements
            .sandbox_policy
            .can_set(&SandboxPolicy::DangerFullAccess),
        Err(ConstraintError::InvalidValue {
            field_name: "sandbox_mode",
            candidate: "DangerFullAccess".into(),
            allowed: "[ReadOnly, WorkspaceWrite]".into(),
            requirement_source: RequirementSource::Unknown,
        })
    );
    assert_eq!(
        requirements
            .sandbox_policy
            .can_set(&SandboxPolicy::ExternalSandbox {
                network_access: NetworkAccess::Restricted,
            }),
        Err(ConstraintError::InvalidValue {
            field_name: "sandbox_mode",
            candidate: "ExternalSandbox".into(),
            allowed: "[ReadOnly, WorkspaceWrite]".into(),
            requirement_source: RequirementSource::Unknown,
        })
    );

    Ok(())
}

#[test]
fn deserialize_allowed_web_search_modes() -> Result<()> {
    let toml_str = r#"
        allowed_web_search_modes = ["cached"]
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements: ConfigRequirements = with_unknown_source(config).try_into()?;

    assert_eq!(requirements.web_search_mode.value(), WebSearchMode::Cached);
    assert!(
        requirements
            .web_search_mode
            .can_set(&WebSearchMode::Disabled)
            .is_ok()
    );
    assert_eq!(
        requirements.web_search_mode.can_set(&WebSearchMode::Live),
        Err(ConstraintError::InvalidValue {
            field_name: "web_search_mode",
            candidate: "Live".into(),
            allowed: "[Disabled, Cached]".into(),
            requirement_source: RequirementSource::Unknown,
        })
    );
    assert!(
        requirements
            .web_search_mode
            .can_set(&WebSearchMode::Cached)
            .is_ok()
    );

    Ok(())
}

#[test]
fn allowed_web_search_modes_allows_disabled() -> Result<()> {
    let toml_str = r#"
        allowed_web_search_modes = ["disabled"]
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements: ConfigRequirements = with_unknown_source(config).try_into()?;

    assert_eq!(
        requirements.web_search_mode.value(),
        WebSearchMode::Disabled
    );
    assert!(
        requirements
            .web_search_mode
            .can_set(&WebSearchMode::Disabled)
            .is_ok()
    );
    assert_eq!(
        requirements.web_search_mode.can_set(&WebSearchMode::Cached),
        Err(ConstraintError::InvalidValue {
            field_name: "web_search_mode",
            candidate: "Cached".into(),
            allowed: "[Disabled]".into(),
            requirement_source: RequirementSource::Unknown,
        })
    );
    Ok(())
}

#[test]
fn allowed_web_search_modes_empty_restricts_to_disabled() -> Result<()> {
    let toml_str = r#"
        allowed_web_search_modes = []
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements: ConfigRequirements = with_unknown_source(config).try_into()?;

    assert_eq!(
        requirements.web_search_mode.value(),
        WebSearchMode::Disabled
    );
    assert!(
        requirements
            .web_search_mode
            .can_set(&WebSearchMode::Disabled)
            .is_ok()
    );
    assert_eq!(
        requirements.web_search_mode.can_set(&WebSearchMode::Cached),
        Err(ConstraintError::InvalidValue {
            field_name: "web_search_mode",
            candidate: "Cached".into(),
            allowed: "[Disabled]".into(),
            requirement_source: RequirementSource::Unknown,
        })
    );
    Ok(())
}

#[test]
fn deserialize_feature_requirements() -> Result<()> {
    let toml_str = r#"
        [features]
        apps = false
        personality = true
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements: ConfigRequirements = with_unknown_source(config).try_into()?;

    assert_eq!(
        requirements.feature_requirements,
        Some(Sourced::new(
            FeatureRequirementsToml {
                entries: BTreeMap::from([
                    ("apps".to_string(), false),
                    ("personality".to_string(), true),
                ]),
            },
            RequirementSource::Unknown,
        ))
    );

    Ok(())
}

#[test]
fn network_requirements_are_preserved_as_constraints_with_source() -> Result<()> {
    let toml_str = r#"
        [experimental_network]
        enabled = true
        allow_upstream_proxy = false
        dangerously_allow_all_unix_sockets = true
        managed_allowed_domains_only = true
        allow_local_binding = false

        [experimental_network.domains]
        "api.example.com" = "allow"
        "*.openai.com" = "allow"
        "blocked.example.com" = "deny"

        [experimental_network.unix_sockets]
        "/tmp/example.sock" = "allow"
    "#;

    let source = RequirementSource::CloudRequirements;
    let mut requirements_with_sources = ConfigRequirementsWithSources::default();
    requirements_with_sources.merge_unset_fields(source.clone(), from_str(toml_str)?);

    let requirements = ConfigRequirements::try_from(requirements_with_sources)?;
    let sourced_network = requirements
        .network
        .expect("network requirements should be preserved as constraints");

    assert_eq!(sourced_network.source, source);
    assert_eq!(sourced_network.value.enabled, Some(true));
    assert_eq!(sourced_network.value.allow_upstream_proxy, Some(false));
    assert_eq!(
        sourced_network.value.dangerously_allow_all_unix_sockets,
        Some(true)
    );
    assert_eq!(
        sourced_network.value.domains.as_ref(),
        Some(&NetworkDomainPermissionsToml {
            entries: BTreeMap::from([
                (
                    "*.openai.com".to_string(),
                    NetworkDomainPermissionToml::Allow,
                ),
                (
                    "api.example.com".to_string(),
                    NetworkDomainPermissionToml::Allow,
                ),
                (
                    "blocked.example.com".to_string(),
                    NetworkDomainPermissionToml::Deny,
                ),
            ]),
        })
    );
    assert_eq!(
        sourced_network.value.managed_allowed_domains_only,
        Some(true)
    );
    assert_eq!(
        sourced_network.value.unix_sockets.as_ref(),
        Some(&NetworkUnixSocketPermissionsToml {
            entries: BTreeMap::from([(
                "/tmp/example.sock".to_string(),
                NetworkUnixSocketPermissionToml::Allow,
            )]),
        })
    );
    assert_eq!(sourced_network.value.allow_local_binding, Some(false));

    Ok(())
}

#[test]
fn network_permission_containers_project_allowed_and_denied_entries() {
    let domains = NetworkDomainPermissionsToml {
        entries: BTreeMap::from([
            (
                "*.openai.com".to_string(),
                NetworkDomainPermissionToml::Allow,
            ),
            (
                "api.example.com".to_string(),
                NetworkDomainPermissionToml::Allow,
            ),
            (
                "blocked.example.com".to_string(),
                NetworkDomainPermissionToml::Deny,
            ),
        ]),
    };
    let unix_sockets = NetworkUnixSocketPermissionsToml {
        entries: BTreeMap::from([
            (
                "/tmp/example.sock".to_string(),
                NetworkUnixSocketPermissionToml::Allow,
            ),
            (
                "/tmp/ignored.sock".to_string(),
                NetworkUnixSocketPermissionToml::None,
            ),
        ]),
    };

    assert_eq!(
        domains.allowed_domains(),
        Some(vec![
            "*.openai.com".to_string(),
            "api.example.com".to_string()
        ])
    );
    assert_eq!(
        domains.denied_domains(),
        Some(vec!["blocked.example.com".to_string()])
    );
    assert_eq!(
        NetworkDomainPermissionsToml {
            entries: BTreeMap::from([(
                "api.example.com".to_string(),
                NetworkDomainPermissionToml::Allow,
            )]),
        }
        .denied_domains(),
        None
    );
    assert_eq!(
        unix_sockets.allow_unix_sockets(),
        vec!["/tmp/example.sock".to_string()]
    );
}

#[test]
fn deserialize_mcp_server_requirements() -> Result<()> {
    let toml_str = r#"
        [mcp_servers.docs.identity]
        command = "praxis-mcp"

        [mcp_servers.remote.identity]
        url = "https://example.com/mcp"
    "#;
    let requirements: ConfigRequirements = with_unknown_source(from_str(toml_str)?).try_into()?;

    assert_eq!(
        requirements.mcp_servers,
        Some(Sourced::new(
            BTreeMap::from([
                (
                    "docs".to_string(),
                    McpServerRequirement {
                        identity: McpServerIdentity::Command {
                            command: "praxis-mcp".to_string(),
                        },
                    },
                ),
                (
                    "remote".to_string(),
                    McpServerRequirement {
                        identity: McpServerIdentity::Url {
                            url: "https://example.com/mcp".to_string(),
                        },
                    },
                ),
            ]),
            RequirementSource::Unknown,
        ))
    );
    Ok(())
}

#[test]
fn deserialize_exec_policy_requirements() -> Result<()> {
    let toml_str = r#"
        [rules]
        prefix_rules = [
            { pattern = [{ token = "rm" }], decision = "forbidden" },
        ]
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements: ConfigRequirements = with_unknown_source(config).try_into()?;
    let policy = requirements.exec_policy.expect("exec policy").value;

    assert_eq!(
        policy.as_ref().check(&tokens(&["rm", "-rf"]), &|_| {
            panic!("rule should match so heuristic should not be called");
        }),
        Evaluation {
            decision: Decision::Forbidden,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: tokens(&["rm"]),
                decision: Decision::Forbidden,
                resolved_program: None,
                justification: None,
            }],
        }
    );

    Ok(())
}

#[test]
fn exec_policy_error_includes_requirement_source() -> Result<()> {
    let toml_str = r#"
        [rules]
        prefix_rules = [
            { pattern = [{ token = "rm" }] },
        ]
    "#;
    let config: ConfigRequirementsToml = from_str(toml_str)?;
    let requirements_toml_file = system_requirements_toml_file_for_test()?;
    let source_location = RequirementSource::SystemRequirementsToml {
        file: requirements_toml_file,
    };

    let mut requirements_with_sources = ConfigRequirementsWithSources::default();
    requirements_with_sources.merge_unset_fields(source_location.clone(), config);
    let err =
        ConfigRequirements::try_from(requirements_with_sources).expect_err("invalid exec policy");

    assert_eq!(
        err,
        ConstraintError::ExecPolicyParse {
            requirement_source: source_location,
            reason: "rules prefix_rule at index 0 is missing a decision".to_string(),
        }
    );

    Ok(())
}

use super::*;

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

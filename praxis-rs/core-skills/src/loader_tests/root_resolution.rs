use super::*;

#[test]
fn skill_roots_from_layer_stack_maps_user_to_user_and_system_cache_and_system_to_admin()
-> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;

    let system_folder = tmp.path().join("etc/praxis");
    let home_folder = tmp.path().join("home");
    let user_folder = home_folder.join("praxis");
    fs::create_dir_all(&system_folder)?;
    fs::create_dir_all(&user_folder)?;

    // The file path doesn't need to exist; it's only used to derive the config folder.
    let system_file = AbsolutePathBuf::from_absolute_path(system_folder.join("config.toml"))?;
    let user_file = AbsolutePathBuf::from_absolute_path(user_folder.join("config.toml"))?;

    let layers = vec![
        ConfigLayerEntry::new(
            ConfigLayerSource::System { file: system_file },
            TomlValue::Table(toml::map::Map::new()),
        ),
        ConfigLayerEntry::new(
            ConfigLayerSource::User { file: user_file },
            TomlValue::Table(toml::map::Map::new()),
        ),
    ];
    let stack = ConfigLayerStack::new(
        layers,
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )?;

    let got = skill_roots_from_layer_stack(&stack, Some(&home_folder))
        .into_iter()
        .map(|root| (root.scope, root.path))
        .collect::<Vec<_>>();

    assert_eq!(
        got,
        vec![
            (SkillScope::User, user_folder.join("skills")),
            (
                SkillScope::User,
                home_folder.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME)
            ),
            (
                SkillScope::System,
                user_folder.join("skills").join(".system")
            ),
            (SkillScope::Admin, system_folder.join("skills")),
        ]
    );

    Ok(())
}

#[test]
fn skill_roots_from_layer_stack_includes_disabled_project_layers() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;

    let home_folder = tmp.path().join("home");
    let user_folder = home_folder.join("praxis");
    fs::create_dir_all(&user_folder)?;

    let project_root = tmp.path().join("repo");
    let dot_praxis = project_root.join(".praxis");
    fs::create_dir_all(&dot_praxis)?;

    let user_file = AbsolutePathBuf::from_absolute_path(user_folder.join("config.toml"))?;
    let project_dot_praxis = AbsolutePathBuf::from_absolute_path(&dot_praxis)?;

    let layers = vec![
        ConfigLayerEntry::new(
            ConfigLayerSource::User { file: user_file },
            TomlValue::Table(toml::map::Map::new()),
        ),
        ConfigLayerEntry::new_disabled(
            ConfigLayerSource::Project {
                dot_praxis_folder: project_dot_praxis,
            },
            TomlValue::Table(toml::map::Map::new()),
            "marked untrusted",
        ),
    ];
    let stack = ConfigLayerStack::new(
        layers,
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )?;

    let got = skill_roots_from_layer_stack(&stack, Some(&home_folder))
        .into_iter()
        .map(|root| (root.scope, root.path))
        .collect::<Vec<_>>();

    assert_eq!(
        got,
        vec![
            (SkillScope::Repo, dot_praxis.join("skills")),
            (SkillScope::User, user_folder.join("skills")),
            (
                SkillScope::User,
                home_folder.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME)
            ),
            (
                SkillScope::System,
                user_folder.join("skills").join(".system")
            ),
        ]
    );

    Ok(())
}

#[test]
fn loads_skills_from_home_agents_dir_for_user_scope() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;

    let home_folder = tmp.path().join("home");
    let user_folder = home_folder.join("praxis");
    fs::create_dir_all(&user_folder)?;

    let user_file = AbsolutePathBuf::from_absolute_path(user_folder.join("config.toml"))?;
    let layers = vec![ConfigLayerEntry::new(
        ConfigLayerSource::User { file: user_file },
        TomlValue::Table(toml::map::Map::new()),
    )];
    let stack = ConfigLayerStack::new(
        layers,
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )?;

    let skill_path = write_skill_at(
        &home_folder.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
        "agents-home",
        "agents-home-skill",
        "from home agents",
    );

    let outcome = load_skills_from_roots(skill_roots_from_layer_stack(&stack, Some(&home_folder)));
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "agents-home-skill".to_string(),
            description: "from home agents".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );

    Ok(())
}

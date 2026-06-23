use super::*;

#[tokio::test]
async fn loads_skills_from_system_cache_when_present() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let work_dir = tempfile::tempdir().expect("tempdir");

    let skill_path = write_system_skill(&praxis_home, "system", "system-skill", "from system");

    let cfg = make_config_for_cwd(&praxis_home, work_dir.path().to_path_buf()).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "system-skill".to_string(),
            description: "from system".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::System,
        }]
    );
}

#[tokio::test]
async fn skill_roots_include_admin_with_lowest_priority() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let cfg = make_config(&praxis_home).await;

    let scopes: Vec<SkillScope> = super::skill_roots(&cfg.config_layer_stack, &cfg.cwd, Vec::new())
        .into_iter()
        .map(|root| root.scope)
        .collect();
    let mut expected = vec![SkillScope::User, SkillScope::System];
    if home_dir().is_some() {
        expected.insert(1, SkillScope::User);
    }
    expected.push(SkillScope::Admin);
    assert_eq!(scopes, expected);
}

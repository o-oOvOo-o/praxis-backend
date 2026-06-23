use super::*;

#[tokio::test]
async fn loads_valid_skill() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "demo-skill", "does things\ncarefully");
    let cfg = make_config(&praxis_home).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "demo-skill".to_string(),
            description: "does things carefully".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn falls_back_to_directory_name_when_skill_name_is_missing() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_raw_skill_at(
        &praxis_home.path().join("skills"),
        "directory-derived",
        "description: fallback name",
    );
    let cfg = make_config(&praxis_home).await;

    let outcome = load_skills_for_test(&cfg);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "directory-derived".to_string(),
            description: "fallback name".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn namespaces_plugin_skills_using_plugin_name() {
    let root = tempfile::tempdir().expect("tempdir");
    let plugin_root = root.path().join("plugins/sample");
    let skill_path = write_raw_skill_at(
        &plugin_root.join("skills"),
        "sample-search",
        "description: search sample data",
    );
    fs::create_dir_all(plugin_root.join(".praxis-plugin")).unwrap();
    fs::write(
        plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{"name":"sample"}"#,
    )
    .unwrap();

    let outcome = load_skills_from_roots([SkillRoot {
        path: plugin_root.join("skills"),
        scope: SkillScope::User,
    }]);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "sample:sample-search".to_string(),
            description: "search sample data".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn loads_short_description_from_metadata() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_dir = praxis_home.path().join("skills/demo");
    fs::create_dir_all(&skill_dir).unwrap();
    let contents = "---\nname: demo-skill\ndescription: long description\nmetadata:\n  short-description: short summary\n---\n\n# Body\n";
    let skill_path = skill_dir.join(SKILLS_FILENAME);
    fs::write(&skill_path, contents).unwrap();

    let cfg = make_config(&praxis_home).await;
    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "demo-skill".to_string(),
            description: "long description".to_string(),
            short_description: Some("short summary".to_string()),
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn enforces_short_description_length_limits() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_dir = praxis_home.path().join("skills/demo");
    fs::create_dir_all(&skill_dir).unwrap();
    let too_long = "x".repeat(MAX_SHORT_DESCRIPTION_LEN + 1);
    let contents = format!(
        "---\nname: demo-skill\ndescription: long description\nmetadata:\n  short-description: {too_long}\n---\n\n# Body\n"
    );
    fs::write(skill_dir.join(SKILLS_FILENAME), contents).unwrap();

    let cfg = make_config(&praxis_home).await;
    let outcome = load_skills_for_test(&cfg);
    assert_eq!(outcome.skills.len(), 0);
    assert_eq!(outcome.errors.len(), 1);
    assert!(
        outcome.errors[0]
            .message
            .contains("invalid metadata.short-description"),
        "expected length error, got: {:?}",
        outcome.errors
    );
}

#[tokio::test]
async fn skips_hidden_and_invalid() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let hidden_dir = praxis_home.path().join("skills/.hidden");
    fs::create_dir_all(&hidden_dir).unwrap();
    fs::write(
        hidden_dir.join(SKILLS_FILENAME),
        "---\nname: hidden\ndescription: hidden\n---\n",
    )
    .unwrap();

    // Invalid because missing closing frontmatter.
    let invalid_dir = praxis_home.path().join("skills/invalid");
    fs::create_dir_all(&invalid_dir).unwrap();
    fs::write(invalid_dir.join(SKILLS_FILENAME), "---\nname: bad").unwrap();

    let cfg = make_config(&praxis_home).await;
    let outcome = load_skills_for_test(&cfg);
    assert_eq!(outcome.skills.len(), 0);
    assert_eq!(outcome.errors.len(), 1);
    assert!(
        outcome.errors[0]
            .message
            .contains("missing YAML frontmatter"),
        "expected frontmatter error"
    );
}

#[tokio::test]
async fn enforces_length_limits() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let max_desc = "\u{1F4A1}".repeat(MAX_DESCRIPTION_LEN);
    write_skill(&praxis_home, "max-len", "max-len", &max_desc);
    let cfg = make_config(&praxis_home).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(outcome.skills.len(), 1);

    let too_long_desc = "\u{1F4A1}".repeat(MAX_DESCRIPTION_LEN + 1);
    write_skill(&praxis_home, "too-long", "too-long", &too_long_desc);
    let outcome = load_skills_for_test(&cfg);
    assert_eq!(outcome.skills.len(), 1);
    assert_eq!(outcome.errors.len(), 1);
    assert!(
        outcome.errors[0].message.contains("invalid description"),
        "expected length error"
    );
}

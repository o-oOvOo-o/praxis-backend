use super::*;

#[tokio::test]
async fn loads_skills_from_repo_root() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    mark_as_git_repo(repo_dir.path());

    let skills_root = repo_dir
        .path()
        .join(REPO_ROOT_CONFIG_DIR_NAME)
        .join(SKILLS_DIR_NAME);
    let skill_path = write_skill_at(&skills_root, "repo", "repo-skill", "from repo");
    let cfg = make_config_for_cwd(&praxis_home, repo_dir.path().to_path_buf()).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "repo-skill".to_string(),
            description: "from repo".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::Repo,
        }]
    );
}

#[tokio::test]
async fn loads_skills_from_agents_dir_without_praxis_dir() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    mark_as_git_repo(repo_dir.path());

    let skill_path = write_skill_at(
        &repo_dir.path().join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
        "agents",
        "agents-skill",
        "from agents",
    );
    let cfg = make_config_for_cwd(&praxis_home, repo_dir.path().to_path_buf()).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "agents-skill".to_string(),
            description: "from agents".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::Repo,
        }]
    );
}

#[tokio::test]
async fn loads_skills_from_all_praxis_dirs_under_project_root() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    mark_as_git_repo(repo_dir.path());

    let nested_dir = repo_dir.path().join("nested/inner");
    fs::create_dir_all(&nested_dir).unwrap();

    let root_skill_path = write_skill_at(
        &repo_dir
            .path()
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "root",
        "root-skill",
        "from root",
    );
    let nested_skill_path = write_skill_at(
        &repo_dir
            .path()
            .join("nested")
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "nested",
        "nested-skill",
        "from nested",
    );

    let cfg = make_config_for_cwd(&praxis_home, nested_dir).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![
            SkillMetadata {
                name: "nested-skill".to_string(),
                description: "from nested".to_string(),
                short_description: None,
                interface: None,
                dependencies: None,
                policy: None,
                path_to_skills_md: normalized(&nested_skill_path),
                scope: SkillScope::Repo,
            },
            SkillMetadata {
                name: "root-skill".to_string(),
                description: "from root".to_string(),
                short_description: None,
                interface: None,
                dependencies: None,
                policy: None,
                path_to_skills_md: normalized(&root_skill_path),
                scope: SkillScope::Repo,
            },
        ]
    );
}

#[tokio::test]
async fn loads_skills_from_praxis_dir_when_not_git_repo() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let work_dir = tempfile::tempdir().expect("tempdir");

    let skill_path = write_skill_at(
        &work_dir
            .path()
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "local",
        "local-skill",
        "from cwd",
    );

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
            name: "local-skill".to_string(),
            description: "from cwd".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::Repo,
        }]
    );
}

#[tokio::test]
async fn deduplicates_by_path_preferring_first_root() {
    let root = tempfile::tempdir().expect("tempdir");

    let skill_path = write_skill_at(root.path(), "dupe", "dupe-skill", "from repo");

    let outcome = load_skills_from_roots([
        SkillRoot {
            path: root.path().to_path_buf(),
            scope: SkillScope::Repo,
        },
        SkillRoot {
            path: root.path().to_path_buf(),
            scope: SkillScope::User,
        },
    ]);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "dupe-skill".to_string(),
            description: "from repo".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::Repo,
        }]
    );
}

#[tokio::test]
async fn keeps_duplicate_names_from_repo_and_user() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    mark_as_git_repo(repo_dir.path());

    let user_skill_path = write_skill(&praxis_home, "user", "dupe-skill", "from user");
    let repo_skill_path = write_skill_at(
        &repo_dir
            .path()
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "repo",
        "dupe-skill",
        "from repo",
    );

    let cfg = make_config_for_cwd(&praxis_home, repo_dir.path().to_path_buf()).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![
            SkillMetadata {
                name: "dupe-skill".to_string(),
                description: "from repo".to_string(),
                short_description: None,
                interface: None,
                dependencies: None,
                policy: None,
                path_to_skills_md: normalized(&repo_skill_path),
                scope: SkillScope::Repo,
            },
            SkillMetadata {
                name: "dupe-skill".to_string(),
                description: "from user".to_string(),
                short_description: None,
                interface: None,
                dependencies: None,
                policy: None,
                path_to_skills_md: normalized(&user_skill_path),
                scope: SkillScope::User,
            },
        ]
    );
}

#[tokio::test]
async fn keeps_duplicate_names_from_nested_praxis_dirs() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    mark_as_git_repo(repo_dir.path());

    let nested_dir = repo_dir.path().join("nested/inner");
    fs::create_dir_all(&nested_dir).unwrap();

    let root_skill_path = write_skill_at(
        &repo_dir
            .path()
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "root",
        "dupe-skill",
        "from root",
    );
    let nested_skill_path = write_skill_at(
        &repo_dir
            .path()
            .join("nested")
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "nested",
        "dupe-skill",
        "from nested",
    );

    let cfg = make_config_for_cwd(&praxis_home, nested_dir).await;
    let outcome = load_skills_for_test(&cfg);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    let root_path = canonicalize_path(&root_skill_path).unwrap_or_else(|_| root_skill_path.clone());
    let nested_path =
        canonicalize_path(&nested_skill_path).unwrap_or_else(|_| nested_skill_path.clone());
    let (first_path, second_path, first_description, second_description) =
        if root_path <= nested_path {
            (root_path, nested_path, "from root", "from nested")
        } else {
            (nested_path, root_path, "from nested", "from root")
        };
    assert_eq!(
        outcome.skills,
        vec![
            SkillMetadata {
                name: "dupe-skill".to_string(),
                description: first_description.to_string(),
                short_description: None,
                interface: None,
                dependencies: None,
                policy: None,
                path_to_skills_md: first_path,
                scope: SkillScope::Repo,
            },
            SkillMetadata {
                name: "dupe-skill".to_string(),
                description: second_description.to_string(),
                short_description: None,
                interface: None,
                dependencies: None,
                policy: None,
                path_to_skills_md: second_path,
                scope: SkillScope::Repo,
            },
        ]
    );
}

#[tokio::test]
async fn repo_skills_search_does_not_escape_repo_root() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let outer_dir = tempfile::tempdir().expect("tempdir");
    let repo_dir = outer_dir.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    let _skill_path = write_skill_at(
        &outer_dir
            .path()
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "outer",
        "outer-skill",
        "from outer",
    );
    mark_as_git_repo(&repo_dir);

    let cfg = make_config_for_cwd(&praxis_home, repo_dir).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(outcome.skills.len(), 0);
}

#[tokio::test]
async fn loads_skills_when_cwd_is_file_in_repo() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    mark_as_git_repo(repo_dir.path());

    let skill_path = write_skill_at(
        &repo_dir
            .path()
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "repo",
        "repo-skill",
        "from repo",
    );
    let file_path = repo_dir.path().join("some-file.txt");
    fs::write(&file_path, "contents").unwrap();

    let cfg = make_config_for_cwd(&praxis_home, file_path).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(
        outcome.skills,
        vec![SkillMetadata {
            name: "repo-skill".to_string(),
            description: "from repo".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::Repo,
        }]
    );
}

#[tokio::test]
async fn non_git_repo_skills_search_does_not_walk_parents() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let outer_dir = tempfile::tempdir().expect("tempdir");
    let nested_dir = outer_dir.path().join("nested/inner");
    fs::create_dir_all(&nested_dir).unwrap();

    write_skill_at(
        &outer_dir
            .path()
            .join(REPO_ROOT_CONFIG_DIR_NAME)
            .join(SKILLS_DIR_NAME),
        "outer",
        "outer-skill",
        "from outer",
    );

    let cfg = make_config_for_cwd(&praxis_home, nested_dir).await;

    let outcome = load_skills_for_test(&cfg);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(outcome.skills.len(), 0);
}

use super::*;

#[tokio::test]
async fn loads_skill_dependencies_metadata_from_yaml() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "dep-skill", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");

    write_skill_metadata_at(
        skill_dir,
        r#"
{
  "dependencies": {
    "tools": [
      {
        "type": "env_var",
        "value": "GITHUB_TOKEN",
        "description": "GitHub API token with repo scopes"
      },
      {
        "type": "mcp",
        "value": "github",
        "description": "GitHub MCP server",
        "transport": "streamable_http",
        "url": "https://example.com/mcp"
      },
      {
        "type": "cli",
        "value": "gh",
        "description": "GitHub CLI"
      },
      {
        "type": "mcp",
        "value": "local-gh",
        "description": "Local GH MCP server",
        "transport": "stdio",
        "command": "gh-mcp"
      }
    ]
  }
}
"#,
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
            name: "dep-skill".to_string(),
            description: "from json".to_string(),
            short_description: None,
            interface: None,
            dependencies: Some(SkillDependencies {
                tools: vec![
                    SkillToolDependency {
                        r#type: "env_var".to_string(),
                        value: "GITHUB_TOKEN".to_string(),
                        description: Some("GitHub API token with repo scopes".to_string()),
                        transport: None,
                        command: None,
                        url: None,
                    },
                    SkillToolDependency {
                        r#type: "mcp".to_string(),
                        value: "github".to_string(),
                        description: Some("GitHub MCP server".to_string()),
                        transport: Some("streamable_http".to_string()),
                        command: None,
                        url: Some("https://example.com/mcp".to_string()),
                    },
                    SkillToolDependency {
                        r#type: "cli".to_string(),
                        value: "gh".to_string(),
                        description: Some("GitHub CLI".to_string()),
                        transport: None,
                        command: None,
                        url: None,
                    },
                    SkillToolDependency {
                        r#type: "mcp".to_string(),
                        value: "local-gh".to_string(),
                        description: Some("Local GH MCP server".to_string()),
                        transport: Some("stdio".to_string()),
                        command: Some("gh-mcp".to_string()),
                        url: None,
                    },
                ],
            }),
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn loads_skill_interface_metadata_from_yaml() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "ui-skill", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");
    let normalized_skill_dir = normalized(skill_dir);

    write_skill_interface_at(
        skill_dir,
        r##"
interface:
  display_name: "UI Skill"
  short_description: "  short    desc   "
  icon_small: "./assets/small-400px.png"
  icon_large: "./assets/large-logo.svg"
  brand_color: "#3B82F6"
  default_prompt: "  default   prompt   "
"##,
    );

    let cfg = make_config(&praxis_home).await;
    let outcome = load_skills_for_test(&cfg);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    let user_skills: Vec<SkillMetadata> = outcome
        .skills
        .into_iter()
        .filter(|skill| skill.scope == SkillScope::User)
        .collect();
    assert_eq!(
        user_skills,
        vec![SkillMetadata {
            name: "ui-skill".to_string(),
            description: "from json".to_string(),
            short_description: None,
            interface: Some(SkillInterface {
                display_name: Some("UI Skill".to_string()),
                short_description: Some("short desc".to_string()),
                icon_small: Some(normalized_skill_dir.join("assets/small-400px.png")),
                icon_large: Some(normalized_skill_dir.join("assets/large-logo.svg")),
                brand_color: Some("#3B82F6".to_string()),
                default_prompt: Some("default prompt".to_string()),
            }),
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(skill_path.as_path()),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn loads_skill_policy_from_yaml() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "policy-skill", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");

    write_skill_metadata_at(
        skill_dir,
        r#"
policy:
  allow_implicit_invocation: false
"#,
    );

    let cfg = make_config(&praxis_home).await;
    let outcome = load_skills_for_test(&cfg);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(outcome.skills.len(), 1);
    assert_eq!(
        outcome.skills[0].policy,
        Some(SkillPolicy {
            allow_implicit_invocation: Some(false),
            products: vec![],
        })
    );
    assert!(outcome.allowed_skills_for_implicit_invocation().is_empty());
}

#[tokio::test]
async fn empty_skill_policy_defaults_to_allow_implicit_invocation() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "policy-empty", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");

    write_skill_metadata_at(
        skill_dir,
        r#"
policy: {}
"#,
    );

    let cfg = make_config(&praxis_home).await;
    let outcome = load_skills_for_test(&cfg);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(outcome.skills.len(), 1);
    assert_eq!(
        outcome.skills[0].policy,
        Some(SkillPolicy {
            allow_implicit_invocation: None,
            products: vec![],
        })
    );
    assert_eq!(
        outcome.allowed_skills_for_implicit_invocation(),
        outcome.skills
    );
}

#[tokio::test]
async fn loads_skill_policy_products_from_yaml() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "policy-products", "from yaml");
    let skill_dir = skill_path.parent().expect("skill dir");

    write_skill_metadata_at(
        skill_dir,
        r#"
policy:
  products:
    - praxis
    - CHATGPT
    - atlas
"#,
    );

    let cfg = make_config(&praxis_home).await;
    let outcome = load_skills_for_test(&cfg);

    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
    assert_eq!(outcome.skills.len(), 1);
    assert_eq!(
        outcome.skills[0].policy,
        Some(SkillPolicy {
            allow_implicit_invocation: None,
            products: vec![Product::praxis(), Product::chatgpt(), Product::atlas()],
        })
    );
}

#[tokio::test]
async fn accepts_icon_paths_under_assets_dir() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "ui-skill", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");
    let normalized_skill_dir = normalized(skill_dir);

    write_skill_interface_at(
        skill_dir,
        r#"
{
  "interface": {
    "display_name": "UI Skill",
    "icon_small": "assets/icon.png",
    "icon_large": "./assets/logo.svg"
  }
}
"#,
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
            name: "ui-skill".to_string(),
            description: "from json".to_string(),
            short_description: None,
            interface: Some(SkillInterface {
                display_name: Some("UI Skill".to_string()),
                short_description: None,
                icon_small: Some(normalized_skill_dir.join("assets/icon.png")),
                icon_large: Some(normalized_skill_dir.join("assets/logo.svg")),
                brand_color: None,
                default_prompt: None,
            }),
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn ignores_invalid_brand_color() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "ui-skill", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");

    write_skill_interface_at(
        skill_dir,
        r#"
{
  "interface": {
    "brand_color": "blue"
  }
}
"#,
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
            name: "ui-skill".to_string(),
            description: "from json".to_string(),
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
async fn ignores_default_prompt_over_max_length() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "ui-skill", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");
    let normalized_skill_dir = normalized(skill_dir);
    let too_long = "x".repeat(MAX_DEFAULT_PROMPT_LEN + 1);

    write_skill_interface_at(
        skill_dir,
        &format!(
            r##"
{{
  "interface": {{
    "display_name": "UI Skill",
    "icon_small": "./assets/small-400px.png",
    "default_prompt": "{too_long}"
  }}
}}
"##
        ),
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
            name: "ui-skill".to_string(),
            description: "from json".to_string(),
            short_description: None,
            interface: Some(SkillInterface {
                display_name: Some("UI Skill".to_string()),
                short_description: None,
                icon_small: Some(normalized_skill_dir.join("assets/small-400px.png")),
                icon_large: None,
                brand_color: None,
                default_prompt: None,
            }),
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

#[tokio::test]
async fn drops_interface_when_icons_are_invalid() {
    let praxis_home = tempfile::tempdir().expect("tempdir");
    let skill_path = write_skill(&praxis_home, "demo", "ui-skill", "from json");
    let skill_dir = skill_path.parent().expect("skill dir");

    write_skill_interface_at(
        skill_dir,
        r#"
{
  "interface": {
    "icon_small": "icon.png",
    "icon_large": "./assets/../logo.svg"
  }
}
"#,
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
            name: "ui-skill".to_string(),
            description: "from json".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: normalized(&skill_path),
            scope: SkillScope::User,
        }]
    );
}

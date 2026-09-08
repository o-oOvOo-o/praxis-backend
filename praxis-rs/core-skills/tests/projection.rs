use praxis_core_skills::model::{
    SkillDependencies, SkillInterface, SkillMetadata, SkillToolDependency,
};
use praxis_protocol::protocol::{SkillMetadata as ProtocolSkillMetadata, SkillScope};
#[test]
fn projection_preserves_all_public_skill_fields() {
    let skill = SkillMetadata {
        name: "review".into(),
        description: "Review code".into(),
        short_description: Some("Review".into()),
        interface: Some(SkillInterface {
            display_name: Some("Reviewer".into()),
            short_description: None,
            icon_small: None,
            icon_large: None,
            brand_color: Some("blue".into()),
            default_prompt: Some("Review changes".into()),
        }),
        dependencies: Some(SkillDependencies {
            tools: vec![SkillToolDependency {
                r#type: "mcp".into(),
                value: "reviewer".into(),
                description: Some("Code review".into()),
                transport: Some("stdio".into()),
                command: Some("review-server".into()),
                url: None,
            }],
        }),
        policy: None,
        path_to_skills_md: "review/SKILL.md".into(),
        scope: SkillScope::User,
    };
    let actual = ProtocolSkillMetadata::from(&skill);
    assert_eq!(
        (
            actual.name,
            actual.description,
            actual.short_description,
            actual.path,
            actual.scope,
            actual.enabled
        ),
        (
            skill.name,
            skill.description,
            skill.short_description,
            skill.path_to_skills_md,
            skill.scope,
            true
        ),
    );
    assert_eq!(
        actual.interface,
        Some(praxis_protocol::protocol::SkillInterface {
            display_name: Some("Reviewer".into()),
            short_description: None,
            icon_small: None,
            icon_large: None,
            brand_color: Some("blue".into()),
            default_prompt: Some("Review changes".into()),
        })
    );
    assert_eq!(
        actual.dependencies,
        Some(praxis_protocol::protocol::SkillDependencies {
            tools: vec![praxis_protocol::protocol::SkillToolDependency {
                r#type: "mcp".into(),
                value: "reviewer".into(),
                description: Some("Code review".into()),
                transport: Some("stdio".into()),
                command: Some("review-server".into()),
                url: None,
            }],
        })
    );
}

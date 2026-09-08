use crate::SkillMetadata;
use praxis_protocol::protocol::SkillDependencies as ProtocolSkillDependencies;
use praxis_protocol::protocol::SkillInterface as ProtocolSkillInterface;
use praxis_protocol::protocol::SkillMetadata as ProtocolSkillMetadata;
use praxis_protocol::protocol::SkillToolDependency as ProtocolSkillToolDependency;

impl From<&SkillMetadata> for ProtocolSkillMetadata {
    fn from(skill: &SkillMetadata) -> Self {
        ProtocolSkillMetadata {
            name: skill.name.clone(),
            description: skill.description.clone(),
            short_description: skill.short_description.clone(),
            interface: skill
                .interface
                .clone()
                .map(|interface| ProtocolSkillInterface {
                    display_name: interface.display_name,
                    short_description: interface.short_description,
                    icon_small: interface.icon_small,
                    icon_large: interface.icon_large,
                    brand_color: interface.brand_color,
                    default_prompt: interface.default_prompt,
                }),
            dependencies: skill.dependencies.clone().map(|dependencies| {
                ProtocolSkillDependencies {
                    tools: dependencies
                        .tools
                        .into_iter()
                        .map(|tool| ProtocolSkillToolDependency {
                            r#type: tool.r#type,
                            value: tool.value,
                            description: tool.description,
                            transport: tool.transport,
                            command: tool.command,
                            url: tool.url,
                        })
                        .collect(),
                }
            }),
            path: skill.path_to_skills_md.clone(),
            scope: skill.scope,
            enabled: true,
        }
    }
}

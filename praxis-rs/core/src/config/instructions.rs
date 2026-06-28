use crate::config_loader::ConfigRequirementsToml;

pub(super) fn guardian_developer_instructions_from_requirements(
    requirements_toml: &ConfigRequirementsToml,
) -> Option<String> {
    requirements_toml
        .guardian_developer_instructions
        .as_deref()
        .and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
}

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::config::AgentRoleConfig;
use toml::Value as TomlValue;

use super::DEFAULT_ROLE_NAME;
use super::built_in;

/// Builds the spawn-agent tool description text from built-in and configured roles.
pub(crate) fn build(user_defined_agent_roles: &BTreeMap<String, AgentRoleConfig>) -> String {
    let built_in_roles = built_in::configs();
    build_from_configs(built_in_roles, user_defined_agent_roles)
}

// This function is not inlined for testing purpose.
fn build_from_configs(
    built_in_roles: &BTreeMap<String, AgentRoleConfig>,
    user_defined_roles: &BTreeMap<String, AgentRoleConfig>,
) -> String {
    let mut seen = BTreeSet::new();
    let mut formatted_roles = Vec::new();
    for (name, declaration) in user_defined_roles {
        if seen.insert(name.as_str()) {
            formatted_roles.push(format_role(name, declaration));
        }
    }
    for (name, declaration) in built_in_roles {
        if seen.insert(name.as_str()) {
            formatted_roles.push(format_role(name, declaration));
        }
    }

    format!(
        "Optional type name for the new agent. If omitted, `{DEFAULT_ROLE_NAME}` is used.\nAvailable roles:\n{}",
        formatted_roles.join("\n"),
    )
}

fn format_role(name: &str, declaration: &AgentRoleConfig) -> String {
    if let Some(description) = &declaration.description {
        let locked_settings_note = declaration
            .config_file
            .as_ref()
            .and_then(|config_file| {
                built_in::config_file_contents(config_file)
                    .map(str::to_owned)
                    .or_else(|| std::fs::read_to_string(config_file).ok())
            })
            .and_then(|contents| toml::from_str::<TomlValue>(&contents).ok())
            .map(|role_toml| {
                let model = role_toml.get("model").and_then(TomlValue::as_str);
                let reasoning_effort = role_toml
                    .get("model_reasoning_effort")
                    .and_then(TomlValue::as_str);

                match (model, reasoning_effort) {
                    (Some(model), Some(reasoning_effort)) => format!(
                        "\n- This role's model is set to `{model}` and its reasoning effort is set to `{reasoning_effort}`. These settings cannot be changed."
                    ),
                    (Some(model), None) => {
                        format!("\n- This role's model is set to `{model}` and cannot be changed.")
                    }
                    (None, Some(reasoning_effort)) => {
                        format!(
                            "\n- This role's reasoning effort is set to `{reasoning_effort}` and cannot be changed."
                        )
                    }
                    (None, None) => String::new(),
                }
            })
            .unwrap_or_default();
        format!("{name}: {{\n{description}{locked_settings_note}\n}}")
    } else {
        format!("{name}: no description")
    }
}

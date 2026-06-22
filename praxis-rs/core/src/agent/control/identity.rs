use crate::agent::role::DEFAULT_ROLE_NAME;
use crate::agent::role::resolve_role_config;
use crate::config::Config;

const AGENT_NAMES: &str = include_str!("../agent_names.txt");

pub(super) fn agent_base_name_candidates(config: &Config, role_name: Option<&str>) -> Vec<String> {
    let role_name = role_name.unwrap_or(DEFAULT_ROLE_NAME);
    if let Some(candidates) =
        resolve_role_config(config, role_name).and_then(|role| role.base_name_candidates.clone())
    {
        return candidates;
    }

    default_agent_base_name_list()
        .into_iter()
        .map(ToOwned::to_owned)
        .collect()
}

fn default_agent_base_name_list() -> Vec<&'static str> {
    AGENT_NAMES
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentDisplayIdentity {
    pub(super) base_name: String,
    pub(super) title: Option<String>,
    pub(super) display_name: String,
}

pub(super) fn build_agent_display_identity(
    base_name: String,
    agent_title: Option<&str>,
) -> AgentDisplayIdentity {
    let title = agent_title
        .and_then(normalize_agent_title)
        .map(|title| strip_redundant_agent_prefix(base_name.as_str(), title.as_str()).to_string())
        .filter(|title| !title.is_empty())
        .filter(|title| !is_redundant_agent_title(base_name.as_str(), title.as_str()));
    let display_name = title
        .as_deref()
        .map(|title| format!("{base_name}-{title}"))
        .unwrap_or_else(|| base_name.clone());
    AgentDisplayIdentity {
        base_name,
        title,
        display_name,
    }
}

fn normalize_agent_title(value: &str) -> Option<String> {
    let collapsed = value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = collapsed
        .trim()
        .trim_matches(|ch| matches!(ch, '-' | '_' | ':' | '：' | '|' | '、' | '，' | ',' | '。'));
    if trimmed.is_empty() {
        return None;
    }

    let mut truncated = String::new();
    for (index, ch) in trimmed.chars().enumerate() {
        if index >= 18 {
            break;
        }
        truncated.push(ch);
    }
    Some(truncated)
}

fn strip_redundant_agent_prefix<'a>(base_name: &str, title: &'a str) -> &'a str {
    for separator in ["-", "－", "—", ":", "："] {
        if let Some(stripped) = title
            .strip_prefix(base_name)
            .and_then(|rest| rest.strip_prefix(separator))
            .map(str::trim)
            .filter(|rest| !rest.is_empty())
        {
            return stripped;
        }
    }

    for separator in ["-", "－", "—", ":", "："] {
        let Some((prefix, rest)) = title.split_once(separator) else {
            continue;
        };
        if !rest.trim().is_empty()
            && prefix.chars().count() <= 4
            && prefix.chars().any(|ch| !ch.is_ascii())
        {
            return rest.trim();
        }
    }
    title
}

fn is_redundant_agent_title(base_name: &str, title: &str) -> bool {
    title == base_name || (base_name.is_ascii() && title.eq_ignore_ascii_case(base_name))
}

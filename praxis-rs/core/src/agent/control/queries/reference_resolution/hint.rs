use super::super::super::*;

impl AgentControl {
    pub(super) fn live_agent_reference_hint(&self) -> String {
        let mut agents = self.state.live_agents();
        agents.sort_by(|left, right| {
            left.agent_display_name
                .as_deref()
                .or(left.agent_base_name.as_deref())
                .unwrap_or_default()
                .cmp(
                    right
                        .agent_display_name
                        .as_deref()
                        .or(right.agent_base_name.as_deref())
                        .unwrap_or_default(),
                )
        });

        if agents.is_empty() {
            return "No live sub-agents are available.".to_string();
        }

        let entries = agents
            .into_iter()
            .take(8)
            .map(|metadata| {
                let mut parts = Vec::new();
                if let Some(base_name) = metadata.agent_base_name.as_deref() {
                    parts.push(format!("base `{base_name}`"));
                }
                if let Some(display_name) = metadata.agent_display_name.as_deref() {
                    parts.push(format!("display `{display_name}`"));
                }
                if let Some(agent_path) = metadata.agent_path.as_ref() {
                    parts.push(format!("path `{}`", agent_path.as_str()));
                }
                if let Some(task_preview) = metadata
                    .last_task_message
                    .as_deref()
                    .and_then(format_live_agent_task_preview)
                {
                    parts.push(format!("task `{task_preview}`"));
                }
                if parts.is_empty()
                    && let Some(agent_id) = metadata.agent_id
                {
                    parts.push(format!("thread `{agent_id}`"));
                }
                parts.join(", ")
            })
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>();

        if entries.is_empty() {
            return "No named live sub-agents are available.".to_string();
        }
        let suffix = if entries.len() == 8 { "; ..." } else { "" };
        format!(
            "Available live sub-agents: {}{}. Use an exact base name, display name, path, or thread id.",
            entries.join("; "),
            suffix
        )
    }
}

fn format_live_agent_task_preview(value: &str) -> Option<String> {
    let collapsed = value
        .chars()
        .map(|ch| {
            if ch.is_control() || ch.is_whitespace() {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut preview = String::new();
    for (index, ch) in trimmed.chars().enumerate() {
        if index >= 160 {
            preview.push_str("...");
            break;
        }
        preview.push(ch);
    }
    Some(preview)
}

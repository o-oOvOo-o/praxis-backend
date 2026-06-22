use super::super::*;

impl AgentControl {
    pub(crate) async fn resolve_agent_reference(
        &self,
        current_thread_id: ThreadId,
        current_session_source: &SessionSource,
        agent_reference: &str,
    ) -> PraxisResult<ThreadId> {
        let state = self.upgrade()?;
        let current_agent_path = current_session_source
            .get_agent_path()
            .unwrap_or_else(AgentPath::root);
        let agent_path = current_agent_path.resolve(agent_reference);
        if let Ok(agent_path) = &agent_path {
            if let Some(thread_id) = self.state.agent_id_for_path(agent_path) {
                return Ok(thread_id);
            }
            let root_thread_id = self
                .resolve_tree_root_thread_id(&state, current_thread_id, current_session_source)
                .await;
            if agent_path.is_root() {
                if state.get_thread(root_thread_id).await.is_ok() {
                    return Ok(root_thread_id);
                }
            } else if let Some(thread_id) = self
                .find_live_agent_by_path(root_thread_id, agent_path)
                .await?
            {
                return Ok(thread_id);
            }
        }

        if let Some(thread_id) = self.state.agent_id_for_human_name(agent_reference)? {
            return Ok(thread_id);
        }

        let reference_hint = self.live_agent_reference_hint();
        match agent_path {
            Ok(agent_path) => Err(PraxisErr::UnsupportedOperation(format!(
                "live agent path `{}` or human name `{}` not found. {}",
                agent_path.as_str(),
                agent_reference.trim(),
                reference_hint
            ))),
            Err(err) => Err(PraxisErr::UnsupportedOperation(format!(
                "agent reference `{}` did not match a live human name and could not be parsed as an agent path: {}. {}",
                agent_reference.trim(),
                err,
                reference_hint
            ))),
        }
    }

    fn live_agent_reference_hint(&self) -> String {
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

    async fn find_live_agent_by_path(
        &self,
        root_thread_id: ThreadId,
        agent_path: &AgentPath,
    ) -> PraxisResult<Option<ThreadId>> {
        let mut matches = self
            .live_agents_in_tree(root_thread_id)
            .await?
            .into_iter()
            .filter_map(|(thread_id, metadata)| {
                (metadata.agent_path.as_ref() == Some(agent_path)).then_some(thread_id)
            })
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(matches.pop()),
            _ => Err(PraxisErr::UnsupportedOperation(format!(
                "multiple live agents found for canonical path `{}`",
                agent_path.as_str()
            ))),
        }
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

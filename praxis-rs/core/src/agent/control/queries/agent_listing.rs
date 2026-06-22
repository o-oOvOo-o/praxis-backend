use super::super::*;
use crate::session_prefix::format_subagent_context_line;

impl AgentControl {
    pub(crate) async fn format_environment_context_subagents(
        &self,
        parent_thread_id: ThreadId,
    ) -> String {
        let Ok(agents) = self.open_thread_spawn_children(parent_thread_id).await else {
            return String::new();
        };

        agents
            .into_iter()
            .map(|(thread_id, metadata)| {
                let reference = metadata
                    .agent_path
                    .as_ref()
                    .map(|agent_path| agent_path.name().to_string())
                    .unwrap_or_else(|| thread_id.to_string());
                format_subagent_context_line(
                    reference.as_str(),
                    metadata.agent_display_name.as_deref(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) async fn list_agents(
        &self,
        current_thread_id: ThreadId,
        current_session_source: &SessionSource,
        path_prefix: Option<&str>,
    ) -> PraxisResult<Vec<ListedAgent>> {
        let state = self.upgrade()?;
        let resolved_prefix = path_prefix
            .map(|prefix| {
                current_session_source
                    .get_agent_path()
                    .unwrap_or_else(AgentPath::root)
                    .resolve(prefix)
                    .map_err(PraxisErr::UnsupportedOperation)
            })
            .transpose()?;

        let root_thread_id = self
            .resolve_tree_root_thread_id(&state, current_thread_id, current_session_source)
            .await;
        let live_agents = self.live_agents_in_tree(root_thread_id).await?;
        let root_path = AgentPath::root();
        let mut agents = Vec::with_capacity(live_agents.len().saturating_add(1));
        if resolved_prefix
            .as_ref()
            .is_none_or(|prefix| agent_matches_prefix(Some(&root_path), prefix))
            && let Ok(root_thread) = state.get_thread(root_thread_id).await
        {
            agents.push(ListedAgent {
                thread_id: root_thread_id,
                agent_name: root_path.to_string(),
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                agent_status: root_thread.agent_status().await,
                last_task_message: Some(ROOT_LAST_TASK_MESSAGE.to_string()),
                recommended_target: root_thread_id.to_string(),
                next_action:
                    "This is the root thread; list_agents omits it from tool output for subagent coordination."
                        .to_string(),
            });
        }

        for (thread_id, metadata) in live_agents {
            if metadata.agent_path.as_ref().is_some_and(AgentPath::is_root) {
                continue;
            }
            if resolved_prefix
                .as_ref()
                .is_some_and(|prefix| !agent_matches_prefix(metadata.agent_path.as_ref(), prefix))
            {
                continue;
            }

            let Ok(thread) = state.get_thread(thread_id).await else {
                continue;
            };
            let agent_name = metadata
                .agent_path
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| thread_id.to_string());
            let last_task_message = metadata.last_task_message.clone();
            let agent_status = thread.agent_status().await;
            let recommended_target = thread_id.to_string();
            let next_action = listed_agent_next_action(&recommended_target, &agent_status);
            agents.push(ListedAgent {
                thread_id,
                agent_name,
                agent_base_name: metadata.agent_base_name.clone(),
                agent_title: metadata.agent_title.clone(),
                agent_display_name: metadata.agent_display_name.clone(),
                agent_role: metadata.agent_role.clone(),
                agent_status,
                last_task_message,
                recommended_target,
                next_action,
            });
        }

        Ok(agents)
    }

    pub(in crate::agent::control) async fn resolve_tree_root_thread_id(
        &self,
        state: &Arc<ThreadManagerInner>,
        current_thread_id: ThreadId,
        current_session_source: &SessionSource,
    ) -> ThreadId {
        let state_db = state
            .get_thread(current_thread_id)
            .await
            .ok()
            .and_then(|thread| thread.state_db());
        resolve_root_thread_id_from_source(
            state,
            current_thread_id,
            current_session_source,
            state_db.as_ref(),
        )
        .await
    }

    pub(in crate::agent::control) async fn live_agents_in_tree(
        &self,
        root_thread_id: ThreadId,
    ) -> PraxisResult<Vec<(ThreadId, AgentMetadata)>> {
        let mut agents = Vec::new();
        let state = self.upgrade()?;
        for thread_id in state.list_thread_ids().await {
            let Ok(thread) = state.get_thread(thread_id).await else {
                continue;
            };
            let snapshot = thread.config_snapshot().await;
            let state_db = thread.state_db();
            let thread_root_id = resolve_root_thread_id_from_source(
                &state,
                thread_id,
                &snapshot.session_source,
                state_db.as_ref(),
            )
            .await;
            if thread_root_id != root_thread_id {
                continue;
            }
            agents.push((
                thread_id,
                merge_live_agent_metadata(
                    self.state.agent_metadata_for_thread(thread_id),
                    thread_id,
                    &snapshot,
                ),
            ));
        }
        agents.sort_by(|left, right| {
            left.1
                .agent_path
                .as_deref()
                .unwrap_or_default()
                .cmp(right.1.agent_path.as_deref().unwrap_or_default())
                .then_with(|| left.0.to_string().cmp(&right.0.to_string()))
        });
        Ok(agents)
    }
}

pub(in crate::agent::control) fn listed_agent_next_action(
    recommended_target: &str,
    status: &AgentStatus,
) -> String {
    match status {
        AgentStatus::PendingInit | AgentStatus::Running => {
            format!(
                "Call wait_agent with target `{recommended_target}` only if this worker result is on the critical path."
            )
        }
        AgentStatus::Completed(_) | AgentStatus::Interrupted => {
            format!(
                "Inspect the result, then use assign_task with target `{recommended_target}` for another turn or close_agent when done."
            )
        }
        AgentStatus::Errored(_) => {
            format!(
                "Inspect the error, then use assign_task with target `{recommended_target}` to retry with a narrower task or close_agent."
            )
        }
        AgentStatus::Shutdown | AgentStatus::NotFound => {
            "Do not target this worker for new work; spawn or resume another worker if needed."
                .to_string()
        }
    }
}

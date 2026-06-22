use super::super::*;

impl AgentControl {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::agent::control) fn prepare_thread_spawn(
        &self,
        reservation: &mut crate::agent::registry::SpawnReservation,
        config: &crate::config::Config,
        parent_thread_id: ThreadId,
        depth: i32,
        agent_path: Option<AgentPath>,
        agent_role: Option<String>,
        preferred_agent_base_name: Option<String>,
        preferred_agent_title: Option<String>,
        preferred_agent_display_name: Option<String>,
        agent_title: Option<&str>,
    ) -> PraxisResult<(SessionSource, AgentMetadata)> {
        if depth == 1 {
            self.state.register_root_thread(parent_thread_id);
        }
        if let Some(agent_path) = agent_path.as_ref() {
            reservation.reserve_agent_path(agent_path)?;
        }
        let candidate_names = agent_base_name_candidates(config, agent_role.as_deref());
        let candidate_name_refs: Vec<&str> = candidate_names.iter().map(String::as_str).collect();
        let preferred_agent_base_name = preferred_agent_base_name
            .as_deref()
            .or_else(|| preferred_agent_display_name.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let base_name = reservation.reserve_agent_base_name_with_preference(
            &candidate_name_refs,
            preferred_agent_base_name,
        )?;
        let requested_title = preferred_agent_title.as_deref().or(agent_title);
        let mut identity = build_agent_display_identity(base_name, requested_title);
        if identity.title.is_none()
            && let Some(display_name) = preferred_agent_display_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
            && display_name != identity.base_name
        {
            identity.display_name = display_name.to_string();
        }
        let session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path: agent_path.clone(),
            agent_base_name: Some(identity.base_name.clone()),
            agent_title: identity.title.clone(),
            agent_display_name: Some(identity.display_name.clone()),
            agent_role: agent_role.clone(),
        });
        let agent_metadata = AgentMetadata {
            agent_id: None,
            agent_path,
            agent_base_name: Some(identity.base_name),
            agent_title: identity.title,
            agent_display_name: Some(identity.display_name),
            agent_role,
            last_task_message: None,
        };
        Ok((session_source, agent_metadata))
    }
}

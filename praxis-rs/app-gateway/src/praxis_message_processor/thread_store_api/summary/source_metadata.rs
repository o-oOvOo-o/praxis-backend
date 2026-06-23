use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;

pub(super) fn with_thread_spawn_agent_metadata(
    source: SessionSource,
    agent_base_name: Option<String>,
    agent_title: Option<String>,
    agent_display_name: Option<String>,
    agent_role: Option<String>,
) -> SessionSource {
    if agent_base_name.is_none()
        && agent_title.is_none()
        && agent_display_name.is_none()
        && agent_role.is_none()
    {
        return source;
    }

    match source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path,
            agent_base_name: existing_agent_base_name,
            agent_title: existing_agent_title,
            agent_display_name: existing_agent_display_name,
            agent_role: existing_agent_role,
        }) => SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path,
            agent_base_name: agent_base_name.or(existing_agent_base_name),
            agent_title: agent_title.or(existing_agent_title),
            agent_display_name: agent_display_name.or(existing_agent_display_name),
            agent_role: agent_role.or(existing_agent_role),
        }),
        _ => source,
    }
}

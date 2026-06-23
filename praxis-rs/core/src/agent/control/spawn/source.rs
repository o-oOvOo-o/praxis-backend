use super::super::*;

pub(super) struct PreparedSpawnSource {
    pub(super) session_source: Option<SessionSource>,
    pub(super) notification_source: Option<SessionSource>,
    pub(super) agent_metadata: AgentMetadata,
}

impl AgentControl {
    pub(super) fn prepare_spawn_source(
        &self,
        reservation: &mut crate::agent::registry::SpawnReservation,
        config: &crate::config::Config,
        session_source: Option<SessionSource>,
        options: &SpawnAgentOptions,
    ) -> PraxisResult<PreparedSpawnSource> {
        let (session_source, agent_metadata) = match session_source {
            Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth,
                agent_path,
                agent_base_name,
                agent_title,
                agent_display_name,
                agent_role,
            })) => {
                let (session_source, agent_metadata) = self.prepare_thread_spawn(
                    reservation,
                    config,
                    parent_thread_id,
                    depth,
                    agent_path,
                    agent_role,
                    agent_base_name,
                    agent_title,
                    agent_display_name,
                    options.agent_title.as_deref(),
                )?;
                (Some(session_source), agent_metadata)
            }
            other => (other, AgentMetadata::default()),
        };
        let notification_source = session_source.clone();

        Ok(PreparedSpawnSource {
            session_source,
            notification_source,
            agent_metadata,
        })
    }
}

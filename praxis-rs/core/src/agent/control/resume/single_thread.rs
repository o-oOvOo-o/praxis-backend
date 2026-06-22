use super::super::*;

impl AgentControl {
    pub(in crate::agent::control::resume) async fn resume_single_thread_from_rollout(
        &self,
        mut config: crate::config::Config,
        thread_id: ThreadId,
        session_source: SessionSource,
    ) -> PraxisResult<ThreadId> {
        if let SessionSource::SubAgent(SubAgentSource::ThreadSpawn { depth, .. }) = &session_source
            && *depth >= config.agent_max_depth
        {
            let _ = config.features.disable(Feature::SpawnCsv);
            let _ = config.features.disable(Feature::Collab);
        }
        let state = self.upgrade()?;
        let mut reservation = self.state.reserve_spawn_slot(config.agent_max_threads)?;
        let (session_source, agent_metadata) = match session_source {
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth,
                agent_path,
                agent_base_name,
                agent_title,
                agent_role: _,
                agent_display_name: _,
            }) => {
                let (
                    resumed_agent_path,
                    resumed_agent_base_name,
                    resumed_agent_title,
                    resumed_agent_display_name,
                    resumed_agent_role,
                ) = if let Some(state_db_ctx) = state_db::get_state_db(&config).await {
                    match state_db_ctx.get_thread(thread_id).await {
                        Ok(Some(metadata)) => (
                            metadata
                                .agent_path
                                .as_deref()
                                .and_then(|agent_path| AgentPath::try_from(agent_path).ok()),
                            metadata.agent_base_name,
                            metadata.agent_title,
                            metadata.agent_display_name,
                            metadata.agent_role,
                        ),
                        Ok(None) | Err(_) => (None, None, None, None, None),
                    }
                } else {
                    (None, None, None, None, None)
                };
                self.prepare_thread_spawn(
                    &mut reservation,
                    &config,
                    parent_thread_id,
                    depth,
                    agent_path.or(resumed_agent_path),
                    resumed_agent_role,
                    agent_base_name.or(resumed_agent_base_name),
                    agent_title.or(resumed_agent_title),
                    resumed_agent_display_name,
                    /*agent_title*/ None,
                )?
            }
            other => (other, AgentMetadata::default()),
        };
        let notification_source = session_source.clone();
        let inherited_shell_snapshot = self
            .inherited_shell_snapshot_for_source(&state, Some(&session_source))
            .await;
        let inherited_exec_policy = self
            .inherited_exec_policy_for_source(&state, Some(&session_source), &config)
            .await;
        let rollout_path =
            match find_thread_path_by_id_str(config.praxis_home.as_path(), &thread_id.to_string())
                .await?
            {
                Some(rollout_path) => rollout_path,
                None => find_archived_thread_path_by_id_str(
                    config.praxis_home.as_path(),
                    &thread_id.to_string(),
                )
                .await?
                .ok_or_else(|| PraxisErr::ThreadNotFound(thread_id))?,
            };

        let resumed_thread = state
            .resume_thread_from_rollout_with_source(
                config,
                rollout_path,
                self.clone(),
                session_source,
                inherited_shell_snapshot,
                inherited_exec_policy,
            )
            .await?;
        let mut agent_metadata = agent_metadata;
        agent_metadata.agent_id = Some(resumed_thread.thread_id);
        reservation.commit(agent_metadata.clone());
        state.notify_thread_created(resumed_thread.thread_id);
        self.persist_thread_spawn_edge_for_source(
            resumed_thread.thread.as_ref(),
            resumed_thread.thread_id,
            Some(&notification_source),
        )
        .await;

        Ok(resumed_thread.thread_id)
    }
}

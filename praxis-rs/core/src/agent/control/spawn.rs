use super::*;

mod fork;

impl AgentControl {
    /// Spawn a new agent thread and submit the initial prompt.
    pub(crate) async fn spawn_agent(
        &self,
        config: crate::config::Config,
        initial_operation: Op,
        session_source: Option<SessionSource>,
    ) -> PraxisResult<ThreadId> {
        Ok(self
            .spawn_agent_internal(
                config,
                initial_operation,
                session_source,
                SpawnAgentOptions::default(),
            )
            .await?
            .thread_id)
    }

    /// Spawn an agent thread with some metadata.
    pub(crate) async fn spawn_agent_with_metadata(
        &self,
        config: crate::config::Config,
        initial_operation: Op,
        session_source: Option<SessionSource>,
        options: SpawnAgentOptions, // TODO(jif) drop with new fork.
    ) -> PraxisResult<LiveAgent> {
        self.spawn_agent_internal(config, initial_operation, session_source, options)
            .await
    }

    async fn spawn_agent_internal(
        &self,
        config: crate::config::Config,
        initial_operation: Op,
        session_source: Option<SessionSource>,
        options: SpawnAgentOptions,
    ) -> PraxisResult<LiveAgent> {
        let state = self.upgrade()?;
        let mut reservation = self.state.reserve_spawn_slot(config.agent_max_threads)?;
        let inherited_shell_snapshot = self
            .inherited_shell_snapshot_for_source(&state, session_source.as_ref())
            .await;
        let inherited_exec_policy = self
            .inherited_exec_policy_for_source(&state, session_source.as_ref(), &config)
            .await;
        let (session_source, mut agent_metadata) = match session_source {
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
                    &mut reservation,
                    &config,
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

        let new_thread = match (session_source, options.fork_mode.as_ref()) {
            (Some(session_source), Some(_)) => {
                self.spawn_forked_thread(
                    &state,
                    config,
                    session_source,
                    &options,
                    inherited_shell_snapshot,
                    inherited_exec_policy,
                )
                .await?
            }
            (Some(session_source), None) => {
                state
                    .spawn_new_thread_with_source(
                        config,
                        self.clone(),
                        session_source,
                        /*persist_extended_history*/ false,
                        /*metrics_service_name*/ None,
                        inherited_shell_snapshot,
                        inherited_exec_policy,
                    )
                    .await?
            }
            (None, _) => state.spawn_new_thread(config, self.clone()).await?,
        };
        agent_metadata.agent_id = Some(new_thread.thread_id);
        reservation.commit(agent_metadata.clone());

        state.notify_thread_created(new_thread.thread_id);

        self.persist_thread_spawn_edge_for_source(
            new_thread.thread.as_ref(),
            new_thread.thread_id,
            notification_source.as_ref(),
        )
        .await;

        self.submit_turn_operation(new_thread.thread_id, initial_operation)
            .await?;

        Ok(LiveAgent {
            thread_id: new_thread.thread_id,
            metadata: agent_metadata,
            status: self.get_status(new_thread.thread_id).await,
        })
    }
}

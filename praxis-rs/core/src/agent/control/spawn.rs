use super::*;

mod completion;
mod fork;
mod runtime_inheritance;
mod source;
mod thread_start;

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
        let inherited_runtime = self
            .inherited_runtime_for_spawn(&state, session_source.as_ref(), &config)
            .await;
        let prepared =
            self.prepare_spawn_source(&mut reservation, &config, session_source, &options)?;
        let new_thread = self
            .start_spawned_thread(
                &state,
                config,
                prepared.session_source,
                &options,
                inherited_runtime,
            )
            .await?;
        let mut agent_metadata = prepared.agent_metadata;
        agent_metadata.agent_id = Some(new_thread.thread_id);
        reservation.commit(agent_metadata.clone());

        self.complete_spawned_thread(
            &state,
            &new_thread,
            prepared.notification_source.as_ref(),
            initial_operation,
        )
        .await?;

        Ok(LiveAgent {
            thread_id: new_thread.thread_id,
            metadata: agent_metadata,
            status: self.get_status(new_thread.thread_id).await,
        })
    }
}

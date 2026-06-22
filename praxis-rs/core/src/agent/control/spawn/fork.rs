use super::super::*;

impl AgentControl {
    pub(super) async fn spawn_forked_thread(
        &self,
        state: &Arc<ThreadManagerInner>,
        config: crate::config::Config,
        session_source: SessionSource,
        options: &SpawnAgentOptions,
        inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
        inherited_exec_policy: Option<Arc<crate::exec_policy::ExecPolicyManager>>,
    ) -> PraxisResult<crate::thread_manager::ThreadSpawnResult> {
        let Some(call_id) = options.fork_parent_spawn_call_id.as_deref() else {
            return Err(PraxisErr::Fatal(
                "spawn_agent fork requires a parent spawn call id".to_string(),
            ));
        };
        let Some(fork_mode) = options.fork_mode.as_ref() else {
            return Err(PraxisErr::Fatal(
                "spawn_agent fork requires a fork mode".to_string(),
            ));
        };
        let SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        }) = &session_source
        else {
            return Err(PraxisErr::Fatal(
                "spawn_agent fork requires a thread-spawn session source".to_string(),
            ));
        };

        let parent_thread_id = *parent_thread_id;
        let parent_thread = state.get_thread(parent_thread_id).await.ok();
        if let Some(parent_thread) = parent_thread.as_ref() {
            parent_thread
                .praxis
                .session
                .ensure_rollout_materialized()
                .await;
            parent_thread.praxis.session.flush_rollout().await;
        }

        let rollout_path = parent_thread
            .as_ref()
            .and_then(|parent_thread| parent_thread.rollout_path())
            .or(find_thread_path_by_id_str(
                config.praxis_home.as_path(),
                &parent_thread_id.to_string(),
            )
            .await?)
            .ok_or_else(|| {
                PraxisErr::Fatal(format!(
                    "parent thread rollout unavailable for fork: {parent_thread_id}"
                ))
            })?;

        let mut forked_rollout_items = RolloutRecorder::get_rollout_history(&rollout_path)
            .await?
            .get_rollout_items();
        if let SpawnAgentForkMode::LastNTurns(last_n_turns) = fork_mode {
            forked_rollout_items =
                truncate_rollout_to_last_n_fork_turns(&forked_rollout_items, *last_n_turns);
        }

        let mut output =
            FunctionCallOutputPayload::from_text(FORKED_SPAWN_AGENT_OUTPUT_MESSAGE.to_string());
        output.success = Some(true);
        forked_rollout_items.push(RolloutItem::ResponseItem(
            ResponseItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output,
            },
        ));

        state
            .fork_thread_with_source(
                config,
                InitialHistory::Forked(forked_rollout_items),
                self.clone(),
                session_source,
                /*persist_extended_history*/ false,
                inherited_shell_snapshot,
                inherited_exec_policy,
            )
            .await
    }
}

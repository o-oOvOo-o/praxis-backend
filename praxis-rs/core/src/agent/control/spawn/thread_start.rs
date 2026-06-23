use super::super::*;
use super::runtime_inheritance::SpawnRuntimeInheritance;

impl AgentControl {
    pub(super) async fn start_spawned_thread(
        &self,
        state: &Arc<ThreadManagerInner>,
        config: crate::config::Config,
        session_source: Option<SessionSource>,
        options: &SpawnAgentOptions,
        inherited_runtime: SpawnRuntimeInheritance,
    ) -> PraxisResult<crate::thread_manager::ThreadSpawnResult> {
        match (session_source, options.fork_mode.as_ref()) {
            (Some(session_source), Some(_)) => {
                self.spawn_forked_thread(
                    state,
                    config,
                    session_source,
                    options,
                    inherited_runtime.inherited_shell_snapshot,
                    inherited_runtime.inherited_exec_policy,
                )
                .await
            }
            (Some(session_source), None) => {
                state
                    .spawn_new_thread_with_source(
                        config,
                        self.clone(),
                        session_source,
                        /*persist_extended_history*/ false,
                        /*metrics_service_name*/ None,
                        inherited_runtime.inherited_shell_snapshot,
                        inherited_runtime.inherited_exec_policy,
                    )
                    .await
            }
            (None, _) => state.spawn_new_thread(config, self.clone()).await,
        }
    }
}

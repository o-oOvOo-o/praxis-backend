use super::super::*;

pub(super) struct SpawnRuntimeInheritance {
    pub(super) inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
    pub(super) inherited_exec_policy: Option<Arc<crate::exec_policy::ExecPolicyManager>>,
}

impl AgentControl {
    pub(super) async fn inherited_runtime_for_spawn(
        &self,
        state: &Arc<ThreadManagerInner>,
        session_source: Option<&SessionSource>,
        config: &crate::config::Config,
    ) -> SpawnRuntimeInheritance {
        let inherited_shell_snapshot = self
            .inherited_shell_snapshot_for_source(state, session_source)
            .await;
        let inherited_exec_policy = self
            .inherited_exec_policy_for_source(state, session_source, config)
            .await;

        SpawnRuntimeInheritance {
            inherited_shell_snapshot,
            inherited_exec_policy,
        }
    }
}

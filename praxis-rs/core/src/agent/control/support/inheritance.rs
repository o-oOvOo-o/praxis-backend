use super::super::*;

impl AgentControl {
    pub(in crate::agent::control) async fn inherited_shell_snapshot_for_source(
        &self,
        state: &Arc<ThreadManagerInner>,
        session_source: Option<&SessionSource>,
    ) -> Option<Arc<ShellSnapshot>> {
        let Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        })) = session_source
        else {
            return None;
        };

        let parent_thread = state.get_thread(*parent_thread_id).await.ok()?;
        parent_thread.praxis.session.user_shell().shell_snapshot()
    }

    pub(in crate::agent::control) async fn inherited_exec_policy_for_source(
        &self,
        state: &Arc<ThreadManagerInner>,
        session_source: Option<&SessionSource>,
        child_config: &crate::config::Config,
    ) -> Option<Arc<crate::exec_policy::ExecPolicyManager>> {
        let Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        })) = session_source
        else {
            return None;
        };

        let parent_thread = state.get_thread(*parent_thread_id).await.ok()?;
        let parent_config = parent_thread.praxis.session.get_config().await;
        if !crate::exec_policy::child_uses_parent_exec_policy(&parent_config, child_config) {
            return None;
        }

        Some(Arc::clone(
            &parent_thread.praxis.session.services.exec_policy,
        ))
    }
}

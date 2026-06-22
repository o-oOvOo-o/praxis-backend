use std::sync::Arc;

use praxis_protocol::protocol::SessionSource;

use crate::agent::AgentControl;
use crate::config::Config;
use crate::shell_snapshot::ShellSnapshot;

use super::ThreadManager;

impl ThreadManager {
    pub(crate) fn agent_control(&self) -> AgentControl {
        AgentControl::new(Arc::downgrade(&self.state))
    }

    pub(crate) async fn agent_control_for_source(
        &self,
        session_source: &SessionSource,
    ) -> AgentControl {
        let SessionSource::SubAgent(praxis_protocol::protocol::SubAgentSource::ThreadSpawn {
            parent_thread_id,
            ..
        }) = session_source
        else {
            return self.agent_control();
        };

        self.state
            .get_thread(*parent_thread_id)
            .await
            .map(|thread| thread.praxis.session.services.agent_control.clone())
            .unwrap_or_else(|_| self.agent_control())
    }

    pub(crate) async fn inherited_shell_snapshot_for_source(
        &self,
        session_source: &SessionSource,
    ) -> Option<Arc<ShellSnapshot>> {
        let SessionSource::SubAgent(praxis_protocol::protocol::SubAgentSource::ThreadSpawn {
            parent_thread_id,
            ..
        }) = session_source
        else {
            return None;
        };

        let parent_thread = self.state.get_thread(*parent_thread_id).await.ok()?;
        parent_thread.praxis.session.user_shell().shell_snapshot()
    }

    pub(crate) async fn inherited_exec_policy_for_source(
        &self,
        session_source: &SessionSource,
        child_config: &Config,
    ) -> Option<Arc<crate::exec_policy::ExecPolicyManager>> {
        let SessionSource::SubAgent(praxis_protocol::protocol::SubAgentSource::ThreadSpawn {
            parent_thread_id,
            ..
        }) = session_source
        else {
            return None;
        };

        let parent_thread = self.state.get_thread(*parent_thread_id).await.ok()?;
        let parent_config = parent_thread.praxis.session.get_config().await;
        if !crate::exec_policy::child_uses_parent_exec_policy(&parent_config, child_config) {
            return None;
        }

        Some(Arc::clone(
            &parent_thread.praxis.session.services.exec_policy,
        ))
    }
}

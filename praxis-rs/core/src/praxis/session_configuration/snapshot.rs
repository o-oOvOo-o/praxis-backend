use std::path::PathBuf;

use crate::praxis_thread::ThreadConfigSnapshot;

use super::types::SessionConfiguration;

impl SessionConfiguration {
    pub(crate) fn praxis_home(&self) -> &PathBuf {
        &self.praxis_home
    }

    pub(in crate::praxis) fn thread_config_snapshot(&self) -> ThreadConfigSnapshot {
        ThreadConfigSnapshot {
            model: self.collaboration_mode.model().to_string(),
            model_provider_id: self.original_config_do_not_use.model_provider_id.clone(),
            service_tier: self.service_tier,
            approval_policy: self.approval_policy.value(),
            approvals_reviewer: self.approvals_reviewer,
            sandbox_policy: self.sandbox_policy.get().clone(),
            cwd: self.cwd.to_path_buf(),
            ephemeral: self.original_config_do_not_use.ephemeral,
            reasoning_effort: self.collaboration_mode.reasoning_effort(),
            personality: self.personality,
            session_source: self.session_source.clone(),
        }
    }
}

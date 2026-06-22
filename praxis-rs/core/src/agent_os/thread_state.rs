use praxis_protocol::ThreadId;

use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;

use super::model::CapabilityProfile;
use super::model::TaskRecord;
use super::model::ThreadRegistryEntry;
use super::state::AgentOsState;

impl AgentOsState {
    pub(super) fn resolve_thread_context(
        &mut self,
        thread_id: ThreadId,
        missing_task_message: &str,
    ) -> PraxisResult<(ThreadRegistryEntry, TaskRecord, CapabilityProfile)> {
        self.ensure_builtin_profiles();
        let thread = self.threads.get(&thread_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "AgentOS thread `{thread_id}` is not registered"
            ))
        })?;
        let task_id = thread
            .current_task_id
            .clone()
            .ok_or_else(|| PraxisErr::UnsupportedOperation(missing_task_message.to_string()))?;
        let task = self.tasks.get(&task_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!("current task `{task_id}` is not registered"))
        })?;
        let profile = self
            .profiles
            .get(&thread.profile_id)
            .cloned()
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "unknown capability profile `{}`",
                    thread.profile_id
                ))
            })?;
        Ok((thread, task, profile))
    }
}

use praxis_protocol::ThreadId;

use crate::agent_os::records::CapabilityProfile;
use crate::agent_os::records::TaskRecord;
use crate::agent_os::records::ThreadRegistryEntry;
use crate::agent_os::state::AgentOsState;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;

impl AgentOsState {
    pub(in crate::agent_os) fn resolve_thread_context(
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

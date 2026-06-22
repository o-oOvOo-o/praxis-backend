use super::super::*;

pub(in crate::agent_os::commands_finish) struct ManagedCommandFinishState {
    pub(in crate::agent_os) command: CommandRecord,
    pub(in crate::agent_os) thread_snapshot: Option<ThreadRegistryEntry>,
    pub(in crate::agent_os) task_snapshot: Option<TaskRecord>,
    pub(in crate::agent_os) lease_ids: Vec<String>,
}

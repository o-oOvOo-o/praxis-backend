use std::path::PathBuf;

use crate::agent_os::model::CommandRecord;
use crate::agent_os::model::TaskRecord;
use crate::agent_os::model::ThreadRegistryEntry;

pub(in crate::agent_os) struct DirtyAuditOutcome {
    pub(in crate::agent_os) command: CommandRecord,
    pub(in crate::agent_os) thread_snapshot: Option<ThreadRegistryEntry>,
    pub(in crate::agent_os) task_snapshot: Option<TaskRecord>,
    pub(in crate::agent_os) dirty_files: Vec<PathBuf>,
    pub(in crate::agent_os) violation_path: Option<PathBuf>,
}

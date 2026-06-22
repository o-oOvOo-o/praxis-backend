use crate::agent_os::model::RuntimeCommandRecord;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RuntimeCommandSummary {
    command_id: String,
    from_thread_id: String,
    to_thread_id: String,
    task_id: Option<String>,
    command_type: String,
    status: String,
    coordinator_epoch: u64,
    fencing_token: u64,
    created_at: String,
    expires_at: String,
}

impl From<RuntimeCommandRecord> for RuntimeCommandSummary {
    fn from(command: RuntimeCommandRecord) -> Self {
        Self {
            command_id: command.command_id,
            from_thread_id: command.from_thread_id.to_string(),
            to_thread_id: command.to_thread_id.to_string(),
            task_id: command.task_id,
            command_type: format!("{:?}", command.command_type),
            status: format!("{:?}", command.status),
            coordinator_epoch: command.coordinator_epoch,
            fencing_token: command.fencing_token,
            created_at: command.created_at.to_rfc3339(),
            expires_at: command.expires_at.to_rfc3339(),
        }
    }
}

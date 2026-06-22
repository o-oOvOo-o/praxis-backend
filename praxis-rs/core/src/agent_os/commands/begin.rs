use super::*;

mod persistence;
mod record;
mod request;
mod state_update;

use request::BeginManagedCommandRequest;

impl AgentOs {
    pub(in crate::agent_os) async fn begin_managed_command(
        &self,
        ticket: &ExecutionTicket,
        command: String,
        argv: &[String],
        cwd: PathBuf,
        process_id: Option<i32>,
        runtime_kind: Option<String>,
        runtime_owner_id: Option<String>,
    ) -> PraxisResult<String> {
        let now = Utc::now();
        let command_id = format!("cmd-{}", Uuid::new_v4());
        let request = BeginManagedCommandRequest {
            ticket,
            command,
            argv,
            cwd,
            process_id,
            runtime_kind,
            runtime_owner_id,
        };
        let record = request.build_record(command_id, now).await?;
        let lease_snapshots = self
            .apply_started_command_state(ticket, &record, now)
            .await?;
        self.persist_started_command(ticket, &record, lease_snapshots)
            .await;
        Ok(record.command_id)
    }
}

use super::super::*;
use super::request::BeginManagedCommandRequest;

impl BeginManagedCommandRequest<'_> {
    pub(super) async fn build_record(
        self,
        command_id: String,
        now: chrono::DateTime<Utc>,
    ) -> PraxisResult<CommandRecord> {
        let command_fingerprint = self.command_fingerprint()?;
        let baseline_dirty_files = if requires_dirty_audit(self.ticket.allowed_intent) {
            audit_git_dirty_files(self.cwd.as_path()).await
        } else {
            Vec::new()
        };
        let baseline_dirty_fingerprints =
            dirty_file_fingerprints(self.cwd.as_path(), &baseline_dirty_files);
        Ok(CommandRecord {
            command_id,
            ticket_id: self.ticket.ticket_id.clone(),
            task_id: self.ticket.task_id.clone(),
            thread_id: self.ticket.thread_id,
            intent: self.ticket.allowed_intent,
            intent_plan_id: self.ticket.intent_plan_id.clone(),
            command_fingerprint,
            raw_command: self.command,
            cwd: self.cwd,
            process_id: self.process_id,
            runtime_kind: self.runtime_kind,
            runtime_owner_id: self.runtime_owner_id,
            started_at: now,
            ended_at: None,
            exit_code: None,
            lease_ids: self.ticket.lease_ids.clone(),
            artifacts: Vec::new(),
            baseline_dirty_files,
            baseline_dirty_fingerprints,
            dirty_files: Vec::new(),
        })
    }
}

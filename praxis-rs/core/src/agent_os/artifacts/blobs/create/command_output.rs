use super::super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn create_command_output_artifact(
        &self,
        command: &CommandRecord,
        command_id: &str,
        exit_code: Option<i32>,
        output_source: &ManagedCommandOutputSource<'_>,
    ) -> PraxisResult<String> {
        let metadata = json!({
            "command_id": command_id,
            "bytes": output_source.byte_len(),
            "exit_code": exit_code,
            "runtime_kind": command.runtime_kind.as_deref(),
            "runtime_owner_id": command.runtime_owner_id.as_deref(),
            "process_id": command.process_id,
        });
        match output_source {
            ManagedCommandOutputSource::Bytes(raw_output) => {
                self.create_blob_artifact(
                    command.task_id.clone(),
                    command.thread_id,
                    artifact_type_for_intent(command.intent),
                    "command-log",
                    output_source.summary(),
                    metadata,
                    "log",
                    raw_output,
                )
                .await
            }
            ManagedCommandOutputSource::Spool { spool, .. } => {
                self.create_blob_artifact_from_spool(
                    command.task_id.clone(),
                    command.thread_id,
                    artifact_type_for_intent(command.intent),
                    "command-log",
                    output_source.summary(),
                    metadata,
                    "log",
                    spool,
                )
                .await
            }
        }
    }
}

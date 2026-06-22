use super::*;

impl AgentOs {
    pub(super) async fn create_finished_command_output_artifact(
        &self,
        command: &CommandRecord,
        command_id: &str,
        exit_code: Option<i32>,
        output_source: &ManagedCommandOutputSource<'_>,
    ) -> PraxisResult<Option<String>> {
        if output_source.is_empty() {
            return Ok(None);
        }
        let artifact_result = self
            .create_command_output_artifact(command, command_id, exit_code, output_source)
            .await;
        if let ManagedCommandOutputSource::Spool { spool, .. } = output_source {
            spool.cleanup().await;
        }
        Ok(Some(artifact_result?))
    }
}

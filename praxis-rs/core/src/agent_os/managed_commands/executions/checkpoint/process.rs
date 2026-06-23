use super::*;

impl AgentOs {
    pub(crate) async fn checkpoint_managed_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self
            .command_id_for_process(process_id, runtime_owner_id)
            .await
        else {
            return Ok(None);
        };
        self.checkpoint_managed_command(command_id.as_str(), raw_output)
            .await
    }

    pub(crate) async fn finish_managed_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        exit_code: Option<i32>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        let Some(command_id) = self
            .command_id_for_process(process_id, runtime_owner_id)
            .await
        else {
            return Ok(None);
        };
        self.finish_managed_command(command_id.as_str(), exit_code, raw_output, true)
            .await
    }
}

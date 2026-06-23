use super::*;

mod dirty_report;
mod finalize;
mod output;
mod state_update;

impl AgentOs {
    pub(in crate::agent_os) async fn finish_managed_command(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        raw_output: &[u8],
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        self.finish_managed_command_with_output_source(
            command_id,
            exit_code,
            ManagedCommandOutputSource::Bytes(raw_output),
            release_leases,
        )
        .await
    }

    pub(in crate::agent_os) async fn finish_managed_command_with_spooled_output(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        output_spool: ExecOutputSpool,
        fallback_raw_output: &[u8],
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        self.finish_managed_command_with_output_source(
            command_id,
            exit_code,
            ManagedCommandOutputSource::Spool {
                spool: output_spool,
                fallback_raw_output,
            },
            release_leases,
        )
        .await
    }

    pub(super) async fn finish_managed_command_with_output_source(
        &self,
        command_id: &str,
        exit_code: Option<i32>,
        output_source: ManagedCommandOutputSource<'_>,
        release_leases: bool,
    ) -> PraxisResult<Option<String>> {
        let mut finish = self
            .mark_managed_command_finished_in_state(command_id, exit_code)
            .await?;

        let artifact_id = self
            .create_finished_command_output_artifact(
                &finish.command,
                command_id,
                exit_code,
                &output_source,
            )
            .await?;
        if let Some(artifact_id) = artifact_id.clone() {
            self.attach_finished_command_artifact(command_id, &mut finish.command, artifact_id)
                .await;
        }

        self.record_finished_command_dirty_audit(
            command_id,
            &mut finish.command,
            &mut finish.thread_snapshot,
            &mut finish.task_snapshot,
        )
        .await?;

        self.finalize_finished_managed_command(
            &finish.command,
            finish.thread_snapshot,
            finish.task_snapshot,
            &finish.lease_ids,
            release_leases,
            artifact_id.clone(),
            exit_code,
        )
        .await?;
        Ok(artifact_id)
    }
}

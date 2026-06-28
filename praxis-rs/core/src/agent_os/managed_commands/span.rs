use std::path::PathBuf;
use std::sync::Arc;

use crate::agent_os::instance::AgentOs;
use crate::error::Result as PraxisResult;
use crate::exec::ExecOutputSpool;

#[derive(Clone)]
pub(crate) struct ManagedCommandSpan {
    pub(super) agent_os: Arc<AgentOs>,
    pub(super) command_id: String,
}

impl ManagedCommandSpan {
    pub(in crate::agent_os) fn new(agent_os: Arc<AgentOs>, command_id: String) -> Self {
        Self {
            agent_os,
            command_id,
        }
    }

    pub(crate) async fn finish_success(&self, raw_output: &[u8]) -> PraxisResult<Option<String>> {
        self.finish(Some(0), raw_output).await
    }

    pub(crate) async fn finish_failure(&self, raw_output: &[u8]) -> PraxisResult<Option<String>> {
        self.finish(Some(-1), raw_output).await
    }

    pub(crate) async fn finish(
        &self,
        exit_code: Option<i32>,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        self.agent_os
            .finish_managed_command(self.command_id.as_str(), exit_code, raw_output, true)
            .await
    }

    pub(crate) async fn finish_with_spooled_output(
        &self,
        exit_code: Option<i32>,
        output_spool: ExecOutputSpool,
        fallback_raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        self.agent_os
            .finish_managed_command_with_spooled_output(
                self.command_id.as_str(),
                exit_code,
                output_spool,
                fallback_raw_output,
                true,
            )
            .await
    }

    pub(crate) async fn record_dirty_files(&self, dirty_files: Vec<PathBuf>) -> PraxisResult<()> {
        self.agent_os
            .record_command_dirty_files(self.command_id.as_str(), dirty_files)
            .await
    }

    pub(crate) async fn attach_process(&self, process_id: i32) -> PraxisResult<()> {
        self.agent_os
            .attach_process_to_managed_command(self.command_id.as_str(), process_id)
            .await
    }

    pub(crate) async fn raw_command(&self) -> Option<String> {
        self.agent_os
            .command_raw_command(self.command_id.as_str())
            .await
    }
}

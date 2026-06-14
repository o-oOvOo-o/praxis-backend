use super::*;

#[derive(Clone)]
pub(crate) struct ManagedCommandSpan {
    pub(super) agent_os: Arc<AgentOs>,
    pub(super) command_id: String,
}

pub(super) struct DirtyAuditOutcome {
    pub(super) command: CommandRecord,
    pub(super) thread_snapshot: Option<ThreadRegistryEntry>,
    pub(super) task_snapshot: Option<TaskRecord>,
    pub(super) dirty_files: Vec<PathBuf>,
    pub(super) violation_path: Option<PathBuf>,
}

pub(super) enum ManagedCommandOutputSource<'a> {
    Bytes(&'a [u8]),
    Spool {
        spool: ExecOutputSpool,
        fallback_raw_output: &'a [u8],
    },
}

impl ManagedCommandOutputSource<'_> {
    pub(super) fn is_empty(&self) -> bool {
        match self {
            Self::Bytes(bytes) => bytes.is_empty(),
            Self::Spool { spool, .. } => spool.is_empty(),
        }
    }

    pub(super) fn byte_len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Spool { spool, .. } => spool.total_bytes(),
        }
    }

    pub(super) fn summary(&self) -> String {
        match self {
            Self::Bytes(bytes) => summarize_output(bytes),
            Self::Spool {
                fallback_raw_output,
                ..
            } => summarize_output(fallback_raw_output),
        }
    }
}

impl ManagedCommandSpan {
    pub(super) fn new(agent_os: Arc<AgentOs>, command_id: String) -> Self {
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

    pub(crate) async fn checkpoint(&self, raw_output: &[u8]) -> PraxisResult<Option<String>> {
        self.agent_os
            .checkpoint_managed_command(self.command_id.as_str(), raw_output)
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

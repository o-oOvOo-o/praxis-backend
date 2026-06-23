use super::source::ExternalAgentSource;
use crate::config::Config;
use std::io;

#[derive(Debug, Clone, Default)]
pub struct ExternalSessionSyncStats {
    pub discovered: usize,
    pub imported: usize,
    pub skipped: usize,
}

impl ExternalSessionSyncStats {
    pub(super) fn discovered(discovered: usize) -> Self {
        Self {
            discovered,
            imported: 0,
            skipped: 0,
        }
    }

    pub(super) fn skip_one(&mut self) {
        self.skipped += 1;
    }

    pub(super) fn import_one(&mut self) {
        self.imported += 1;
    }
}

pub async fn sync_external_agent_sessions_to_praxis_home(
    source: ExternalAgentSource,
    config: &Config,
) -> io::Result<ExternalSessionSyncStats> {
    match source {
        ExternalAgentSource::Codex => super::codex::sync_sessions_to_store(config).await,
        ExternalAgentSource::Cursor => super::cursor::sync_sessions_to_store(config).await,
    }
}

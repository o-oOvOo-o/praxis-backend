use super::ExternalAgentSource;
use crate::config::Config;
use async_trait::async_trait;
use std::io;

#[derive(Debug, Clone, Default)]
pub struct ExternalSessionSyncStats {
    pub discovered: usize,
    pub imported: usize,
    pub skipped: usize,
}

impl ExternalSessionSyncStats {
    pub(crate) fn discovered(discovered: usize) -> Self {
        Self {
            discovered,
            imported: 0,
            skipped: 0,
        }
    }

    pub(crate) fn skip_one(&mut self) {
        self.skipped += 1;
    }

    pub(crate) fn import_one(&mut self) {
        self.imported += 1;
    }
}

pub struct ExternalSessionSyncContext<'a> {
    pub config: &'a Config,
}

#[async_trait]
pub trait ExternalAgentSessionProvider {
    fn source(&self) -> ExternalAgentSource;

    async fn sync_to_store(
        &self,
        ctx: ExternalSessionSyncContext<'_>,
    ) -> io::Result<ExternalSessionSyncStats>;
}

pub async fn sync_external_agent_sessions_to_praxis_home(
    source: ExternalAgentSource,
    config: &Config,
) -> io::Result<ExternalSessionSyncStats> {
    match source {
        ExternalAgentSource::Codex => Ok(ExternalSessionSyncStats::default()),
        ExternalAgentSource::Cursor => {
            super::cursor::CursorSessionProvider
                .sync_to_store(ExternalSessionSyncContext { config })
                .await
        }
    }
}

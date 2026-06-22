mod convert;
mod db;
mod extract;
mod locator;
mod model;
mod provider;

use super::provider::ExternalSessionSyncStats;
use super::source::ExternalAgentSource;
use crate::config::Config;
use std::io;

pub(super) const SOURCE: ExternalAgentSource = ExternalAgentSource::Cursor;

pub(super) async fn sync_sessions_to_store(
    config: &Config,
) -> io::Result<ExternalSessionSyncStats> {
    provider::CursorSessionProvider.sync_to_store(config).await
}

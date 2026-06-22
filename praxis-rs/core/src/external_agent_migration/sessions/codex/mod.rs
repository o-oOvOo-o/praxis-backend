use super::provider::ExternalSessionSyncStats;
use std::io;

pub(super) fn sync_sessions_to_store() -> io::Result<ExternalSessionSyncStats> {
    Ok(ExternalSessionSyncStats::default())
}

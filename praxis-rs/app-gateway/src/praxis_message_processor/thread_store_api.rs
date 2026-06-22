mod facade;
mod history;
mod list;
mod paths;
mod summary;

pub(crate) use facade::ThreadStore;
pub(in crate::praxis_message_processor) use history::ThreadHistorySource;
pub(in crate::praxis_message_processor) use list::ThreadStoreListQuery;
pub(in crate::praxis_message_processor) use summary::ThreadStoreSummary;
#[cfg(test)]
pub(in crate::praxis_message_processor) use summary::extract_rollout_summary;
#[cfg(test)]
pub(in crate::praxis_message_processor) use summary::summary_from_state_db_metadata;

#[cfg(test)]
use std::path::Path;

#[cfg(test)]
pub(in crate::praxis_message_processor) async fn read_summary_from_rollout(
    path: &Path,
    fallback_provider: &str,
) -> std::io::Result<ThreadStoreSummary> {
    ThreadStore::read_rollout_summary(path, fallback_provider).await
}

mod history;
mod list;
mod paths;
mod projection;
mod summary;

pub(in crate::praxis_message_processor) use history::ThreadHistoryPageReadError;
pub(in crate::praxis_message_processor) use history::ThreadHistorySource;
pub(in crate::praxis_message_processor) use history::ThreadTurnHydration;
pub(in crate::praxis_message_processor) use list::ThreadStoreListPage;
pub(in crate::praxis_message_processor) use list::ThreadStoreListQuery;
pub(crate) use projection::ThreadProjection;
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
    ThreadProjection::read_rollout_summary(path, fallback_provider).await
}

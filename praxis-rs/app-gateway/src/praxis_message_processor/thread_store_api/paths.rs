use praxis_protocol::ThreadId;
use std::path::Path;
use std::path::PathBuf;

pub(super) async fn read_thread_history_cwd(
    store: &praxis_rollout::ThreadStore<'_, praxis_core::config::Config>,
    thread_id: Option<ThreadId>,
    rollout_path: &Path,
) -> Option<PathBuf> {
    store.read_history_cwd(thread_id, rollout_path).await
}

pub(super) async fn find_thread_rollout_path(
    store: &praxis_rollout::ThreadStore<'_, praxis_core::config::Config>,
    thread_id: ThreadId,
    archived_only: Option<bool>,
) -> std::io::Result<Option<PathBuf>> {
    store.find_rollout_path(thread_id, archived_only).await
}

pub(super) async fn thread_exists(
    store: &praxis_rollout::ThreadStore<'_, praxis_core::config::Config>,
    thread_id: ThreadId,
    archived_only: Option<bool>,
) -> std::io::Result<bool> {
    store.thread_exists(thread_id, archived_only).await
}

pub(super) async fn write_thread_name(
    store: &praxis_rollout::ThreadStore<'_, praxis_core::config::Config>,
    thread_id: ThreadId,
    name: &str,
) -> std::io::Result<()> {
    store.write_thread_name(thread_id, name).await
}

pub(super) async fn resolve_thread_name(
    store: &praxis_rollout::ThreadStore<'_, praxis_core::config::Config>,
    thread_id: ThreadId,
) -> Option<String> {
    store.resolve_thread_name(thread_id).await
}

use praxis_core::config::Config;
use praxis_protocol::ThreadId;
use std::path::Path;
use std::path::PathBuf;

pub(super) async fn read_thread_history_cwd(
    config: &Config,
    thread_id: Option<ThreadId>,
    rollout_path: &Path,
) -> Option<PathBuf> {
    praxis_rollout::ThreadDirectory::open(config)
        .await
        .read_history_cwd(thread_id, rollout_path)
        .await
}

pub(super) async fn find_thread_rollout_path(
    config: &Config,
    thread_id: ThreadId,
    archived_only: Option<bool>,
) -> std::io::Result<Option<PathBuf>> {
    praxis_rollout::ThreadDirectory::open(config)
        .await
        .find_rollout_path(thread_id, archived_only)
        .await
}

pub(super) async fn thread_exists(
    config: &Config,
    thread_id: ThreadId,
    archived_only: Option<bool>,
) -> std::io::Result<bool> {
    praxis_rollout::ThreadDirectory::open(config)
        .await
        .thread_exists(thread_id, archived_only)
        .await
}

pub(super) async fn write_thread_name(
    config: &Config,
    thread_id: ThreadId,
    name: &str,
) -> std::io::Result<()> {
    praxis_rollout::ThreadDirectory::open(config)
        .await
        .write_thread_name(thread_id, name)
        .await
}

pub(super) async fn resolve_thread_name(config: &Config, thread_id: ThreadId) -> Option<String> {
    praxis_rollout::ThreadDirectory::open(config)
        .await
        .resolve_thread_name(thread_id)
        .await
}

pub(super) async fn resolve_thread_name_from_home(
    praxis_home: &Path,
    thread_id: ThreadId,
) -> Option<String> {
    let state_db = praxis_rollout::state_db::open_if_present(praxis_home, "").await;
    praxis_rollout::ThreadNameResolver::new(state_db.as_deref())
        .resolve_name(thread_id)
        .await
}

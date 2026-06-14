use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum ThreadRolloutScope {
    Active,
    Any,
    Archived,
}

impl ThreadRolloutScope {
    fn archived_filter(self) -> Option<bool> {
        match self {
            Self::Active => Some(false),
            Self::Any => None,
            Self::Archived => Some(true),
        }
    }

    fn missing_message(self, thread_id: ThreadId) -> String {
        match self {
            Self::Archived => format!("no archived rollout found for thread id {thread_id}"),
            Self::Active | Self::Any => format!("no rollout found for thread id {thread_id}"),
        }
    }

    fn locate_failed_message(self, thread_id: ThreadId, err: impl std::fmt::Display) -> String {
        match self {
            Self::Archived => format!("failed to locate archived thread id {thread_id}: {err}"),
            Self::Active | Self::Any => format!("failed to locate thread id {thread_id}: {err}"),
        }
    }
}

pub(super) async fn find_thread_rollout_path(
    config: &Config,
    thread_id: ThreadId,
    scope: ThreadRolloutScope,
) -> Result<PathBuf, JSONRPCErrorError> {
    let directory = praxis_rollout::ThreadDirectory::open(config).await;
    match directory
        .find_rollout_path(thread_id, scope.archived_filter())
        .await
    {
        Ok(Some(path)) => Ok(path),
        Ok(None) => Err(crate::json_rpc_error::invalid_request(
            scope.missing_message(thread_id),
        )),
        Err(err) => Err(crate::json_rpc_error::invalid_request(
            scope.locate_failed_message(thread_id, err),
        )),
    }
}

pub(super) async fn find_thread_rollout_path_or_not_found(
    config: &Config,
    thread_id: ThreadId,
) -> Result<PathBuf, JSONRPCErrorError> {
    let directory = praxis_rollout::ThreadDirectory::open(config).await;
    match directory.find_rollout_path(thread_id, None).await {
        Ok(Some(path)) => Ok(path),
        Ok(None) => Err(crate::json_rpc_error::invalid_request(format!(
            "thread not found: {thread_id}"
        ))),
        Err(err) => Err(crate::json_rpc_error::internal_error(format!(
            "failed to locate thread id {thread_id}: {err}"
        ))),
    }
}

pub(super) async fn resolve_thread_rollout_path(
    config: &Config,
    thread_id: ThreadId,
    preferred_path: Option<PathBuf>,
) -> Result<PathBuf, JSONRPCErrorError> {
    if let Some(path) = preferred_path
        && path.exists()
    {
        return Ok(path);
    }

    find_thread_rollout_path(config, thread_id, ThreadRolloutScope::Any).await
}

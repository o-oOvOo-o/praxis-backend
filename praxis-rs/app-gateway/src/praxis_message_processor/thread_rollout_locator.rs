use super::thread_store_api::ThreadStore;
use praxis_app_gateway_protocol::{JSONRPCErrorError, PraxisErrorInfo, TurnError};
use praxis_core::config::Config;
use praxis_protocol::ThreadId;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub(super) enum ThreadRolloutScope {
    Active,
    Any,
    Archived,
}

impl ThreadRolloutScope {
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

#[derive(Clone, Copy)]
enum ThreadRolloutLookupMode {
    Scoped(ThreadRolloutScope),
    NotFound,
    NoRollout,
}

impl ThreadRolloutLookupMode {
    async fn lookup_rollout_path(
        self,
        config: &Config,
        thread_id: ThreadId,
    ) -> std::io::Result<Option<PathBuf>> {
        let store = ThreadStore::new(config);
        match self {
            Self::Scoped(ThreadRolloutScope::Active) => {
                store.find_active_rollout_path(thread_id).await
            }
            Self::Scoped(ThreadRolloutScope::Any) | Self::NotFound | Self::NoRollout => {
                store.find_any_rollout_path(thread_id).await
            }
            Self::Scoped(ThreadRolloutScope::Archived) => {
                store.find_archived_rollout_path(thread_id).await
            }
        }
    }

    fn missing_error(self, thread_id: ThreadId) -> JSONRPCErrorError {
        let error = match self {
            Self::Scoped(scope) => {
                crate::json_rpc_error::invalid_request(scope.missing_message(thread_id))
            }
            Self::NotFound => {
                crate::json_rpc_error::invalid_request(format!("thread not found: {thread_id}"))
            }
            Self::NoRollout => crate::json_rpc_error::invalid_request(format!(
                "no rollout found for thread id {thread_id}"
            )),
        };
        thread_rollout_unavailable(error)
    }

    fn locate_error(self, thread_id: ThreadId, err: impl std::fmt::Display) -> JSONRPCErrorError {
        let error = match self {
            Self::Scoped(scope) => {
                crate::json_rpc_error::invalid_request(scope.locate_failed_message(thread_id, err))
            }
            Self::NotFound | Self::NoRollout => crate::json_rpc_error::internal_error(format!(
                "failed to locate thread id {thread_id}: {err}"
            )),
        };
        thread_rollout_unavailable(error)
    }
}

fn thread_rollout_unavailable(mut error: JSONRPCErrorError) -> JSONRPCErrorError {
    error.data = serde_json::to_value(TurnError {
        message: error.message.clone(),
        praxis_error_info: Some(PraxisErrorInfo::ThreadRolloutUnavailable),
        additional_details: None,
    })
    .ok();
    error
}

pub(super) async fn find_thread_rollout_path(
    config: &Config,
    thread_id: ThreadId,
    scope: ThreadRolloutScope,
) -> Result<PathBuf, JSONRPCErrorError> {
    find_required_thread_rollout_path(config, thread_id, ThreadRolloutLookupMode::Scoped(scope))
        .await
}

pub(super) async fn find_thread_rollout_path_or_not_found(
    config: &Config,
    thread_id: ThreadId,
) -> Result<PathBuf, JSONRPCErrorError> {
    find_required_thread_rollout_path(config, thread_id, ThreadRolloutLookupMode::NotFound).await
}

pub(super) async fn find_thread_rollout_path_or_no_rollout(
    config: &Config,
    thread_id: ThreadId,
) -> Result<PathBuf, JSONRPCErrorError> {
    find_required_thread_rollout_path(config, thread_id, ThreadRolloutLookupMode::NoRollout).await
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

async fn find_required_thread_rollout_path(
    config: &Config,
    thread_id: ThreadId,
    mode: ThreadRolloutLookupMode,
) -> Result<PathBuf, JSONRPCErrorError> {
    match mode.lookup_rollout_path(config, thread_id).await {
        Ok(Some(path)) => Ok(path),
        Ok(None) => Err(mode.missing_error(thread_id)),
        Err(err) => Err(mode.locate_error(thread_id, err)),
    }
}

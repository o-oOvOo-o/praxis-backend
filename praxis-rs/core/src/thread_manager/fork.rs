use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::W3cTraceContext;

use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::rollout::RolloutRecorder;

use super::ThreadManager;
use super::ThreadSpawnResult;

mod history;
mod snapshot_mode;

use history::fork_initial_history;
pub use snapshot_mode::ThreadForkSnapshot;

impl ThreadManager {
    /// Fork an existing thread by snapshotting rollout history according to
    /// `snapshot` and starting a new thread with identical configuration
    /// (unless overridden by the caller's `config`). The new thread will have
    /// a fresh id.
    pub async fn fork_thread<S>(
        &self,
        snapshot: S,
        config: Config,
        path: PathBuf,
        persist_extended_history: bool,
        parent_trace: Option<W3cTraceContext>,
    ) -> PraxisResult<ThreadSpawnResult>
    where
        S: Into<ThreadForkSnapshot>,
    {
        self.fork_thread_with_requested_id(
            snapshot,
            config,
            path,
            persist_extended_history,
            parent_trace,
            None,
        )
        .await
    }

    /// Fork a thread with an optional identity assigned by the owning API.
    pub async fn fork_thread_with_requested_id<S>(
        &self,
        snapshot: S,
        config: Config,
        path: PathBuf,
        persist_extended_history: bool,
        parent_trace: Option<W3cTraceContext>,
        requested_thread_id: Option<ThreadId>,
    ) -> PraxisResult<ThreadSpawnResult>
    where
        S: Into<ThreadForkSnapshot>,
    {
        let snapshot = snapshot.into();
        let history = RolloutRecorder::get_rollout_history(&path).await?;
        let history = fork_initial_history(snapshot, history);
        Box::pin(self.state.spawn_thread_with_requested_id(
            config,
            history,
            Arc::clone(&self.state.auth_manager),
            self.agent_control(),
            Vec::new(),
            persist_extended_history,
            /*metrics_service_name*/ None,
            parent_trace,
            /*user_shell_override*/ None,
            requested_thread_id,
        ))
        .await
    }
}

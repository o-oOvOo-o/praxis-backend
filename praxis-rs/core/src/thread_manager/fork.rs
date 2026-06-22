use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::W3cTraceContext;

use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::rollout::RolloutRecorder;

use super::ThreadManager;
use super::ThreadSpawnResult;
use super::fork_snapshot::append_interrupted_boundary;
use super::fork_snapshot::snapshot_turn_state;
use super::fork_snapshot::truncate_before_nth_user_message;

// TODO(ccunningham): Add an explicit non-interrupting live-turn snapshot once
// core can represent sampling boundaries directly instead of relying on
// whichever items happened to be persisted mid-turn.
//
// Two likely future variants:
// - `TruncateToLastSamplingBoundary` for callers that want a coherent fork from
//   the last stable model boundary without synthesizing an interrupt.
// - `WaitUntilNextSamplingBoundary` (or similar) for callers that prefer to
//   fork after the next sampling boundary rather than interrupting immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadForkSnapshot {
    /// Fork a committed prefix ending strictly before the nth user message.
    ///
    /// When `n` is within range, this cuts before that 0-based user-message
    /// boundary. When `n` is out of range and the source thread is currently
    /// mid-turn, this instead cuts before the active turn's opening boundary
    /// so the fork drops the unfinished turn suffix. When `n` is out of range
    /// and the source thread is already at a turn boundary, this returns the
    /// full committed history unchanged.
    TruncateBeforeNthUserMessage(usize),

    /// Fork the current persisted history as if the source thread had been
    /// interrupted now.
    ///
    /// If the persisted snapshot ends mid-turn, this appends the same
    /// `<turn_aborted>` marker produced by a real interrupt. If the snapshot is
    /// already at a turn boundary, this returns the current persisted history
    /// unchanged.
    Interrupted,
}

/// Preserve legacy `fork_thread(usize, ...)` callsites by mapping them to the
/// existing truncate-before-nth-user-message snapshot mode.
impl From<usize> for ThreadForkSnapshot {
    fn from(value: usize) -> Self {
        Self::TruncateBeforeNthUserMessage(value)
    }
}

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
        let snapshot = snapshot.into();
        let history = RolloutRecorder::get_rollout_history(&path).await?;
        let snapshot_state = snapshot_turn_state(&history);
        let history = match snapshot {
            ThreadForkSnapshot::TruncateBeforeNthUserMessage(nth_user_message) => {
                truncate_before_nth_user_message(history, nth_user_message, &snapshot_state)
            }
            ThreadForkSnapshot::Interrupted => {
                let history = match history {
                    InitialHistory::New => InitialHistory::New,
                    InitialHistory::Forked(history) => InitialHistory::Forked(history),
                    InitialHistory::Resumed(resumed) => InitialHistory::Forked(resumed.history),
                };
                if snapshot_state.ends_mid_turn {
                    append_interrupted_boundary(history, snapshot_state.active_turn_id)
                } else {
                    history
                }
            }
        };
        Box::pin(self.state.spawn_thread(
            config,
            history,
            Arc::clone(&self.state.auth_manager),
            self.agent_control(),
            Vec::new(),
            persist_extended_history,
            /*metrics_service_name*/ None,
            parent_trace,
            /*user_shell_override*/ None,
        ))
        .await
    }
}

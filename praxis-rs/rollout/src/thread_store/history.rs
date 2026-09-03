use std::io;
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::ResumedHistory;
use praxis_protocol::protocol::RolloutItem;
use tracing::info;

use super::native_codec;
use super::scan_items;
use super::thread_id_from_rollout_path;

#[derive(Clone, Debug)]
pub struct ThreadHistoryReader {
    native_store: praxis_thread_store::ThreadStore,
}

impl ThreadHistoryReader {
    pub fn from_praxis_home(praxis_home: impl Into<PathBuf>) -> Self {
        Self {
            native_store: praxis_thread_store::ThreadStore::from_praxis_home(praxis_home.into()),
        }
    }

    pub(crate) fn from_native_store(native_store: praxis_thread_store::ThreadStore) -> Self {
        Self { native_store }
    }

    pub async fn fold_items<S, F>(
        &self,
        rollout_path: &Path,
        mut state: S,
        mut fold: F,
    ) -> io::Result<S>
    where
        S: Send + 'static,
        F: FnMut(&mut S, RolloutItem) + Send + 'static,
    {
        let thread_id = thread_id_from_rollout_path(rollout_path)
            .ok_or_else(|| IoError::other("rollout path does not contain a thread id"))?;
        let native_thread_id = native_thread_id(thread_id)?;
        if self.native_store.thread_exists(native_thread_id).await {
            let folded = self
                .native_store
                .fold_thread_events(
                    native_thread_id,
                    (state, fold, 0usize, 0usize),
                    move |(state, fold, native_events, foreign_events), event| {
                        if let praxis_thread_store_contracts::ThreadEventBody::NativeAgentEventRecorded {
                            payload,
                            ..
                        } = &event.body
                        {
                            *native_events = native_events.saturating_add(1);
                            match native_codec::decode_item(payload) {
                                Some(item) => fold(state, item),
                                None => *foreign_events = foreign_events.saturating_add(1),
                            }
                        }
                    },
                )
                .await
                .map_err(native_store_error)?
                .ok_or_else(|| IoError::other("native thread disappeared during history read"))?;
            let (native_state, native_fold, native_events, foreign_events) = folded;
            if native_events != 0 {
                if foreign_events != 0 {
                    return Err(IoError::other(format!(
                        "native thread contains {foreign_events} events from an incompatible schema"
                    )));
                }
                return Ok(native_state);
            }
            state = native_state;
            fold = native_fold;
        }

        let (parsed_thread_id, parse_errors) =
            scan_items(rollout_path, |item| fold(&mut state, item)).await?;
        if parse_errors != 0 || parsed_thread_id != Some(thread_id) {
            return Err(IoError::other(format!(
                "invalid rollout projection: {parse_errors} parse errors or mismatched thread id"
            )));
        }
        Ok(state)
    }

    pub async fn read_items(&self, rollout_path: &Path) -> io::Result<Vec<RolloutItem>> {
        self.fold_items(rollout_path, Vec::new(), |items, item| items.push(item))
            .await
    }

    pub async fn read_initial_history(&self, rollout_path: &Path) -> io::Result<InitialHistory> {
        let conversation_id = thread_id_from_rollout_path(rollout_path)
            .ok_or_else(|| IoError::other("rollout path does not contain a thread id"))?;
        let history = self.read_items(rollout_path).await?;
        if history.is_empty() {
            return Ok(InitialHistory::New);
        }
        info!("Hydrated persisted thread from {rollout_path:?}");
        Ok(InitialHistory::Resumed(ResumedHistory {
            conversation_id,
            history,
            rollout_path: rollout_path.to_path_buf(),
        }))
    }
}

fn native_thread_id(thread_id: ThreadId) -> io::Result<praxis_thread_store_contracts::ThreadId> {
    praxis_thread_store_contracts::ThreadId::parse(thread_id.to_string().as_str())
        .map_err(native_store_error)
}

fn native_store_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("native thread store: {error}"))
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;

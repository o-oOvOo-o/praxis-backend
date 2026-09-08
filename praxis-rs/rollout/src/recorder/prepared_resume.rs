use super::NativeRolloutWriter;
use crate::thread_store::native_codec;
use praxis_protocol::protocol::{InitialHistory, ResumedHistory};
use praxis_thread_store::ThreadStore;
use praxis_thread_store_contracts::ThreadEventBody;
use std::io;
use std::path::{Path, PathBuf};

/// Owns the history and writer recovered together from one native journal pass.
pub struct PreparedRolloutResume {
    history: InitialHistory,
    writer: PreparedRolloutWriter,
}

/// Transfers exclusive recovered writer ownership into a resumed recorder.
pub struct PreparedRolloutWriter {
    path: PathBuf,
    writer: NativeRolloutWriter,
}

impl PreparedRolloutResume {
    pub async fn open(praxis_home: &Path, path: &Path) -> io::Result<Option<Self>> {
        let conversation_id = crate::thread_store::thread_id_from_rollout_path(path)
            .ok_or_else(|| io::Error::other("rollout path does not contain a thread id"))?;
        let thread_id =
            praxis_thread_store_contracts::ThreadId::parse(&conversation_id.to_string())
                .map_err(io::Error::other)?;
        let store = ThreadStore::from_praxis_home(praxis_home);
        if !store.thread_exists(thread_id).await {
            return Ok(None);
        }
        let (thread, (history, incompatible), _) = store
            .open_thread_and_fold(
                thread_id,
                |_| (Vec::new(), 0usize),
                |(history, incompatible), event| {
                    if let ThreadEventBody::NativeAgentEventRecorded { payload, .. } = event.body {
                        match native_codec::decode_item(&payload) {
                            Some(item) => history.push(item),
                            None => *incompatible += 1,
                        }
                    }
                },
            )
            .await
            .map_err(io::Error::other)?;
        if incompatible != 0 {
            return Err(io::Error::other(format!(
                "native thread contains {incompatible} events from an incompatible schema"
            )));
        }
        if history.is_empty() {
            return Ok(None);
        }
        let locator = history.iter().find_map(|item| match item {
            praxis_protocol::protocol::RolloutItem::SessionMeta(meta)
                if meta.meta.id == conversation_id =>
            {
                Some(meta)
            }
            _ => None,
        });
        let writer =
            NativeRolloutWriter::from_recovered(thread, conversation_id, path, locator).await?;
        Ok(Some(Self {
            history: InitialHistory::Resumed(ResumedHistory {
                conversation_id,
                history,
                rollout_path: path.to_path_buf(),
            }),
            writer: PreparedRolloutWriter {
                path: path.to_path_buf(),
                writer,
            },
        }))
    }

    pub fn into_parts(self) -> (InitialHistory, PreparedRolloutWriter) {
        (self.history, self.writer)
    }
}

impl PreparedRolloutWriter {
    pub(super) fn into_native_writer(self, path: &Path) -> io::Result<NativeRolloutWriter> {
        if self.path != path {
            return Err(io::Error::other(
                "prepared writer belongs to a different rollout path",
            ));
        }
        Ok(self.writer)
    }
}

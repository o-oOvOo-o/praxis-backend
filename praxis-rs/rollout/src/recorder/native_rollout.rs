use std::io;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_thread_store::CommitMode;
use praxis_thread_store::LiveThreadStore;
use praxis_thread_store::ThreadStore;
use praxis_thread_store_contracts::AgentEventRoute;
use praxis_thread_store_contracts::ContentRef;
use praxis_thread_store_contracts::ThreadActor;
use praxis_thread_store_contracts::ThreadCommand;
use praxis_thread_store_contracts::ThreadEventBody;
use serde::Deserialize;
use serde::Serialize;

use super::jsonl_writer::encode_rollout_line;

const NATIVE_ROLLOUT_SCHEMA: &str = "praxis.rollout-item.v1";

pub(super) struct NativeRolloutInit {
    pub praxis_home: PathBuf,
    pub thread_id: ThreadId,
    pub source: String,
    pub workspace: String,
}

pub(super) struct NativeRolloutWriter {
    thread: LiveThreadStore,
    next_sequence: u64,
}

#[derive(Serialize)]
struct StoredRolloutItemRef<'a> {
    schema: &'static str,
    item: &'a RolloutItem,
}

#[derive(Deserialize)]
struct StoredRolloutItem {
    schema: String,
    item: RolloutItem,
}

impl NativeRolloutWriter {
    pub(super) async fn resume(praxis_home: PathBuf, rollout_path: &Path) -> io::Result<Self> {
        let thread_id = crate::thread_store::thread_id_from_rollout_path(rollout_path)
            .ok_or_else(|| io::Error::other("rollout path does not contain a thread id"))?;
        let native_thread_id = native_thread_id(thread_id)?;
        let store = ThreadStore::from_praxis_home(praxis_home.clone());
        if !store.thread_exists(native_thread_id).await {
            let session_meta = crate::list::read_session_meta_line(rollout_path).await?;
            return Self::open(
                NativeRolloutInit {
                    praxis_home,
                    thread_id,
                    source: session_meta.meta.source.to_string(),
                    workspace: session_meta.meta.cwd.to_string_lossy().into_owned(),
                },
                rollout_path,
            )
            .await;
        }

        let thread = store
            .open_thread(native_thread_id)
            .await
            .map_err(store_error)?;
        let next_sequence = thread
            .next_agent_event_sequence()
            .await
            .map_err(store_error)?;
        let mut writer = Self {
            thread,
            next_sequence,
        };
        if next_sequence > 1 {
            writer.reconcile_projection(rollout_path).await?;
        } else {
            writer.import_projection(rollout_path, thread_id).await?;
        }
        Ok(writer)
    }

    pub(super) async fn open(init: NativeRolloutInit, rollout_path: &Path) -> io::Result<Self> {
        let thread_id = native_thread_id(init.thread_id)?;
        let store = ThreadStore::from_praxis_home(init.praxis_home);
        let existed = store.thread_exists(thread_id).await;
        let projection_exists = tokio::fs::try_exists(rollout_path).await?;
        let imported_items = if !existed && projection_exists {
            Some(read_projection(rollout_path, init.thread_id).await?)
        } else {
            None
        };
        let thread = store.open_thread(thread_id).await.map_err(store_error)?;
        thread
            .ensure_created(init.source, init.workspace, None)
            .await
            .map_err(store_error)?;
        let next_sequence = thread
            .next_agent_event_sequence()
            .await
            .map_err(store_error)?;
        let mut writer = Self {
            thread,
            next_sequence,
        };
        if existed && next_sequence > 1 {
            writer.reconcile_projection(rollout_path).await?;
        } else if let Some(items) = imported_items {
            writer.append(&items).await?;
        } else if projection_exists {
            writer
                .import_projection(rollout_path, init.thread_id)
                .await?;
        }
        Ok(writer)
    }

    async fn import_projection(
        &mut self,
        rollout_path: &Path,
        expected_thread_id: ThreadId,
    ) -> io::Result<()> {
        let items = read_projection(rollout_path, expected_thread_id).await?;
        self.append(&items).await
    }

    pub(super) async fn append(&mut self, items: &[RolloutItem]) -> io::Result<()> {
        for item in items {
            let sequence = self.next_sequence;
            self.thread
                .execute(
                    ThreadActor::Runtime,
                    Some(format!("rollout:{sequence}")),
                    ThreadCommand::RecordNativeAgentEvent {
                        agent_sequence: sequence,
                        event_id: format!("rollout:{sequence}"),
                        turn_id: None,
                        route: route(item),
                        payload: encode_item(item)?,
                    },
                    CommitMode::Buffered,
                )
                .await
                .map_err(store_error)?;
            self.next_sequence = sequence.saturating_add(1);
        }
        if !items.is_empty() {
            self.thread.sync().await.map_err(store_error)?;
        }
        Ok(())
    }

    pub(super) async fn sync(&self) -> io::Result<()> {
        self.thread.sync().await.map_err(store_error)?;
        Ok(())
    }

    async fn reconcile_projection(&mut self, rollout_path: &Path) -> io::Result<()> {
        if !tokio::fs::try_exists(rollout_path).await? {
            return self.rebuild_projection(rollout_path).await;
        }
        let mut projected_items = 0u64;
        let (projected_thread_id, parse_errors) =
            crate::thread_store::scan_items(rollout_path, |_| {
                projected_items = projected_items.saturating_add(1);
            })
            .await?;
        let native_items = self.next_sequence.saturating_sub(1);
        let expected_thread_id = ThreadId::from_string(&self.thread.thread_id().to_string()).ok();
        if parse_errors != 0
            || projected_thread_id != expected_thread_id
            || projected_items != native_items
        {
            self.rebuild_projection(rollout_path).await?;
        }
        Ok(())
    }

    async fn rebuild_projection(&self, rollout_path: &Path) -> io::Result<()> {
        let parent = rollout_path
            .parent()
            .ok_or_else(|| io::Error::other("rollout projection path has no parent"))?;
        tokio::fs::create_dir_all(parent).await?;
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        let (file, temporary_path) = temporary.into_parts();
        let rebuild = self
            .thread
            .fold_all(ProjectionRebuild::new(file), |rebuild, event| {
                if rebuild.error.is_some() {
                    return;
                }
                if let ThreadEventBody::NativeAgentEventRecorded { payload, .. } = &event.body {
                    match decode_item(payload) {
                        Some(item) => rebuild.write(&item),
                        None => {
                            rebuild.foreign_events = rebuild.foreign_events.saturating_add(1);
                        }
                    }
                }
            })
            .await
            .map_err(store_error)?;
        if rebuild.foreign_events != 0 {
            return Err(io::Error::other(format!(
                "native thread contains {} events from an incompatible schema",
                rebuild.foreign_events
            )));
        }
        rebuild.finish()?;
        temporary_path
            .persist(rollout_path)
            .map_err(|error| error.error)?;
        Ok(())
    }
}

struct ProjectionRebuild {
    writer: BufWriter<std::fs::File>,
    foreign_events: usize,
    error: Option<io::Error>,
}

impl ProjectionRebuild {
    fn new(file: std::fs::File) -> Self {
        Self {
            writer: BufWriter::new(file),
            foreign_events: 0,
            error: None,
        }
    }

    fn write(&mut self, item: &RolloutItem) {
        let result =
            encode_rollout_line(item).and_then(|line| self.writer.write_all(line.as_bytes()));
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    fn finish(mut self) -> io::Result<()> {
        if let Some(error) = self.error {
            return Err(error);
        }
        self.writer.flush()?;
        self.writer.get_ref().sync_all()
    }
}

pub(super) fn encode_item(item: &RolloutItem) -> io::Result<ContentRef> {
    Ok(ContentRef::InlineText {
        text: serde_json::to_string(&StoredRolloutItemRef {
            schema: NATIVE_ROLLOUT_SCHEMA,
            item,
        })?,
    })
}

pub(super) fn decode_item(content: &ContentRef) -> Option<RolloutItem> {
    let ContentRef::InlineText { text } = content else {
        return None;
    };
    let stored: StoredRolloutItem = serde_json::from_str(text).ok()?;
    (stored.schema == NATIVE_ROLLOUT_SCHEMA).then_some(stored.item)
}

fn route(item: &RolloutItem) -> AgentEventRoute {
    match item {
        RolloutItem::EventMsg(EventMsg::TurnStarted(_)) => AgentEventRoute::TurnStarted,
        RolloutItem::EventMsg(EventMsg::UserMessage(_)) => AgentEventRoute::UserMessage,
        RolloutItem::EventMsg(EventMsg::AgentMessage(_)) => AgentEventRoute::AssistantMessage,
        RolloutItem::SessionMeta(_) => AgentEventRoute::Other,
        RolloutItem::ResponseItem(_)
        | RolloutItem::Compacted(_)
        | RolloutItem::TurnContext(_)
        | RolloutItem::EventMsg(_) => AgentEventRoute::Transcript,
    }
}

fn store_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("native thread store: {error}"))
}

fn native_thread_id(thread_id: ThreadId) -> io::Result<praxis_thread_store_contracts::ThreadId> {
    praxis_thread_store_contracts::ThreadId::parse(thread_id.to_string().as_str())
        .map_err(store_error)
}

async fn read_projection(
    rollout_path: &Path,
    expected_thread_id: ThreadId,
) -> io::Result<Vec<RolloutItem>> {
    let (items, parsed_thread_id, parse_errors) =
        crate::thread_store::read_items(rollout_path).await?;
    if parse_errors != 0 || parsed_thread_id != Some(expected_thread_id) {
        return Err(io::Error::other(format!(
            "cannot import rollout projection with {parse_errors} parse errors or mismatched thread id"
        )));
    }
    Ok(items)
}

#[cfg(test)]
#[path = "native_rollout_tests.rs"]
mod tests;

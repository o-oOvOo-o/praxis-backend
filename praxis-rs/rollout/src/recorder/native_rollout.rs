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
use praxis_thread_store_contracts::ThreadActor;
use praxis_thread_store_contracts::ThreadCommand;
use praxis_thread_store_contracts::ThreadEventBody;

use super::jsonl_writer::encode_rollout_line;
use super::native_metadata::METADATA_GENERATION;
use super::native_metadata::NativeMetadataDelta;
use super::native_metadata::NativeRolloutMetadata;
use crate::thread_store::native_codec::decode_item;
use crate::thread_store::native_codec::encode_item;

pub(super) struct NativeRolloutInit {
    pub praxis_home: PathBuf,
    pub thread_id: ThreadId,
    pub source: String,
    pub workspace: String,
}

pub(crate) struct NativeRolloutWriter {
    thread: LiveThreadStore,
    thread_id: ThreadId,
    next_sequence: u64,
    metadata: NativeRolloutMetadata,
    metadata_generation: u32,
}

impl NativeRolloutWriter {
    pub(crate) async fn resume(praxis_home: PathBuf, rollout_path: &Path) -> io::Result<Self> {
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
        let metadata_generation = thread
            .agent_event_metadata_generation()
            .await
            .map_err(store_error)?;
        let metadata = if metadata_generation >= METADATA_GENERATION {
            thread
                .summary()
                .await
                .map_err(store_error)?
                .as_ref()
                .map(NativeRolloutMetadata::from_summary)
                .unwrap_or_default()
        } else {
            recover_metadata(&thread, thread_id, rollout_path).await?
        };
        let mut writer = Self {
            thread,
            thread_id,
            next_sequence,
            metadata,
            metadata_generation,
        };
        if next_sequence > 1 {
            writer.reconcile_locator(rollout_path).await?;
        } else {
            writer.import_projection(rollout_path, thread_id).await?;
        }
        writer
            .reconcile_metadata_if_needed(NativeMetadataDelta::default())
            .await?;
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
            .ensure_created(init.source, init.workspace.clone(), None)
            .await
            .map_err(store_error)?;
        let next_sequence = thread
            .next_agent_event_sequence()
            .await
            .map_err(store_error)?;
        let projection_updated_at = if projection_exists {
            rollout_modified_unix_ms(rollout_path).await
        } else {
            None
        };
        let metadata_generation = thread
            .agent_event_metadata_generation()
            .await
            .map_err(store_error)?;
        let metadata = if metadata_generation >= METADATA_GENERATION {
            thread
                .summary()
                .await
                .map_err(store_error)?
                .as_ref()
                .map(NativeRolloutMetadata::from_summary)
                .unwrap_or_default()
        } else if existed {
            recover_metadata(&thread, init.thread_id, rollout_path).await?
        } else {
            NativeRolloutMetadata {
                updated_at_unix_ms: projection_updated_at,
                workspace: Some(init.workspace),
                ..NativeRolloutMetadata::default()
            }
        };
        let mut writer = Self {
            thread,
            thread_id: init.thread_id,
            next_sequence,
            metadata,
            metadata_generation,
        };
        if existed && next_sequence > 1 {
            writer.reconcile_locator(rollout_path).await?;
        } else if let Some(items) = imported_items {
            writer.append(&items).await?;
        } else if projection_exists {
            writer
                .import_projection(rollout_path, init.thread_id)
                .await?;
        }
        if !existed && projection_exists && writer.next_sequence > 1 {
            writer.reconcile_locator(rollout_path).await?;
        }
        if writer.next_sequence > 1 {
            writer
                .reconcile_metadata_if_needed(NativeMetadataDelta::default())
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
        self.metadata.updated_at_unix_ms = rollout_modified_unix_ms(rollout_path).await;
        self.append(&items).await
    }

    pub(super) async fn append(&mut self, items: &[RolloutItem]) -> io::Result<()> {
        let mut metadata_delta = NativeMetadataDelta::default();
        for item in items {
            metadata_delta.merge(self.metadata.apply(self.thread_id, item));
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
        self.reconcile_metadata_if_needed(metadata_delta).await?;
        if !items.is_empty() {
            self.thread.sync().await.map_err(store_error)?;
        }
        Ok(())
    }

    async fn reconcile_metadata_if_needed(&mut self, delta: NativeMetadataDelta) -> io::Result<()> {
        let upgrading = self.metadata_generation < METADATA_GENERATION;
        if !upgrading
            && !delta.workspace
            && !delta.preview
            && !delta.resume_config
            && !delta.dynamic_tools
        {
            return Ok(());
        }
        if (upgrading || delta.workspace)
            && let Some(workspace) = self.metadata.workspace.clone()
        {
            self.execute_metadata(ThreadCommand::SetWorkspace { workspace })
                .await?;
        }
        if upgrading || delta.preview {
            self.execute_metadata(ThreadCommand::SetPreview {
                preview: self.metadata.preview.clone(),
                first_user_message: self.metadata.first_user_message.clone(),
            })
            .await?;
        }
        if upgrading || delta.resume_config {
            self.execute_metadata(ThreadCommand::SetResumeConfig {
                model: self.metadata.resume_config.model.clone(),
                model_provider: self.metadata.resume_config.model_provider.clone(),
                reasoning_effort: self.metadata.resume_config.reasoning_effort.clone(),
            })
            .await?;
        }
        if (upgrading || delta.dynamic_tools)
            && let Some(tools) = self.metadata.dynamic_tools.clone()
        {
            self.execute_metadata(ThreadCommand::SetDynamicTools { tools })
                .await?;
        }
        if upgrading {
            self.execute_metadata(ThreadCommand::ReconcileAgentEventTimeline {
                generation: METADATA_GENERATION,
                created_at_unix_ms: self.metadata.created_at_unix_ms,
                updated_at_unix_ms: self.metadata.updated_at_unix_ms,
            })
            .await?;
            self.metadata_generation = METADATA_GENERATION;
        }
        Ok(())
    }

    async fn execute_metadata(&self, command: ThreadCommand) -> io::Result<()> {
        self.thread
            .execute(
                ThreadActor::Runtime,
                Some(format!("rollout-metadata:{METADATA_GENERATION}")),
                command,
                CommitMode::Buffered,
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }

    pub(crate) async fn sync(&self) -> io::Result<()> {
        self.thread.sync().await.map_err(store_error)?;
        Ok(())
    }

    pub(crate) async fn set_name(&self, name: String) -> io::Result<()> {
        self.thread
            .execute(
                ThreadActor::Runtime,
                None,
                ThreadCommand::SetName { name: Some(name) },
                CommitMode::Durable,
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }

    pub(crate) async fn set_archived(&self, archived: bool) -> io::Result<()> {
        self.thread
            .execute(
                ThreadActor::Runtime,
                None,
                ThreadCommand::SetArchived { archived },
                CommitMode::Durable,
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }

    async fn reconcile_locator(&mut self, rollout_path: &Path) -> io::Result<()> {
        if !tokio::fs::try_exists(rollout_path).await? {
            return self.rebuild_locator(rollout_path).await;
        }
        let mut projected_items = 0u64;
        let (projected_thread_id, parse_errors) =
            crate::thread_store::scan_items(rollout_path, |_| {
                projected_items = projected_items.saturating_add(1);
            })
            .await?;
        let expected_thread_id = ThreadId::from_string(&self.thread.thread_id().to_string()).ok();
        if parse_errors != 0 || projected_thread_id != expected_thread_id || projected_items != 1 {
            self.rebuild_locator(rollout_path).await?;
        }
        Ok(())
    }

    async fn rebuild_locator(&self, rollout_path: &Path) -> io::Result<()> {
        let parent = rollout_path
            .parent()
            .ok_or_else(|| io::Error::other("rollout projection path has no parent"))?;
        tokio::fs::create_dir_all(parent).await?;
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        let (file, temporary_path) = temporary.into_parts();
        let expected_thread_id = ThreadId::from_string(&self.thread.thread_id().to_string())
            .map_err(|error| io::Error::other(error.to_string()))?;
        let rebuild = self
            .thread
            .fold_all(LocatorRebuild::new(file), move |rebuild, event| {
                if rebuild.error.is_some() {
                    return;
                }
                if !rebuild.wrote_locator
                    && let ThreadEventBody::NativeAgentEventRecorded { payload, .. } = &event.body
                {
                    match decode_item(payload) {
                        Some(item) => {
                            if matches!(
                                &item,
                                RolloutItem::SessionMeta(meta)
                                    if meta.meta.id == expected_thread_id
                            ) {
                                rebuild.write(&item);
                            }
                        }
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
        if !rebuild.wrote_locator {
            return Err(io::Error::other(
                "native thread does not contain a session metadata locator",
            ));
        }
        rebuild.finish()?;
        temporary_path
            .persist(rollout_path)
            .map_err(|error| error.error)?;
        Ok(())
    }
}

async fn recover_metadata(
    thread: &LiveThreadStore,
    expected_thread_id: ThreadId,
    rollout_path: &Path,
) -> io::Result<NativeRolloutMetadata> {
    let mut metadata = thread
        .fold_all(NativeRolloutMetadata::default(), move |metadata, event| {
            if let ThreadEventBody::NativeAgentEventRecorded { payload, .. } = &event.body
                && let Some(item) = decode_item(payload)
            {
                metadata.apply(expected_thread_id, &item);
            }
        })
        .await
        .map_err(store_error)?;
    metadata.updated_at_unix_ms = rollout_modified_unix_ms(rollout_path).await;
    Ok(metadata)
}

async fn rollout_modified_unix_ms(rollout_path: &Path) -> Option<i64> {
    tokio::fs::metadata(rollout_path)
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

struct LocatorRebuild {
    writer: BufWriter<std::fs::File>,
    foreign_events: usize,
    error: Option<io::Error>,
    wrote_locator: bool,
}

impl LocatorRebuild {
    fn new(file: std::fs::File) -> Self {
        Self {
            writer: BufWriter::new(file),
            foreign_events: 0,
            error: None,
            wrote_locator: false,
        }
    }

    fn write(&mut self, item: &RolloutItem) {
        let result =
            encode_rollout_line(item).and_then(|line| self.writer.write_all(line.as_bytes()));
        if let Err(error) = result {
            self.error = Some(error);
        } else {
            self.wrote_locator = true;
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

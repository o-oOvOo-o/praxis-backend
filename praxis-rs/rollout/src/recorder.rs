//! Persist native Praxis threads and a minimal `.jsonl` identity locator.

use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;

use chrono::Utc;
use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::models::BaseInstructions;
use praxis_utils_string::truncate_middle_chars;
use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tracing::warn;

use super::SESSIONS_SUBDIR;
use super::metadata;
use super::policy::EventPersistenceMode;
use super::policy::is_persisted_response_item;
use crate::config::RolloutConfigView;
use crate::default_client::originator;
use crate::state_db;
use crate::state_db::StateDbHandle;
use praxis_git_utils::collect_git_info;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::GitInfo as ProtocolGitInfo;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionMeta;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_state::StateRuntime;
use praxis_state::ThreadMetadataBuilder;

mod jsonl_writer;
mod native_metadata;
mod native_rollout;

use jsonl_writer::JsonlWriter;
use native_rollout::NativeRolloutInit;
pub(crate) use native_rollout::NativeRolloutWriter;

/// Records session facts in the native journal while retaining a stable path locator.
#[derive(Clone)]
pub struct RolloutRecorder {
    tx: Sender<RolloutCmd>,
    pub(crate) rollout_path: PathBuf,
    state_db: Option<StateDbHandle>,
    event_persistence_mode: EventPersistenceMode,
}

#[derive(Clone)]
pub enum RolloutRecorderParams {
    Create {
        conversation_id: ThreadId,
        forked_from_id: Option<ThreadId>,
        source: SessionSource,
        base_instructions: BaseInstructions,
        dynamic_tools: Vec<DynamicToolSpec>,
        event_persistence_mode: EventPersistenceMode,
    },
    Resume {
        path: PathBuf,
        event_persistence_mode: EventPersistenceMode,
    },
}

enum RolloutCmd {
    AddItems(Vec<RolloutItem>),
    SetThreadName {
        name: String,
        ack: oneshot::Sender<std::io::Result<()>>,
    },
    Persist {
        ack: oneshot::Sender<std::io::Result<()>>,
    },
    /// Ensure all prior writes are processed; respond when flushed.
    Flush {
        ack: oneshot::Sender<std::io::Result<()>>,
    },
    Shutdown {
        ack: oneshot::Sender<std::io::Result<()>>,
    },
}

impl RolloutRecorderParams {
    pub fn new(
        conversation_id: ThreadId,
        forked_from_id: Option<ThreadId>,
        source: SessionSource,
        base_instructions: BaseInstructions,
        dynamic_tools: Vec<DynamicToolSpec>,
        event_persistence_mode: EventPersistenceMode,
    ) -> Self {
        Self::Create {
            conversation_id,
            forked_from_id,
            source,
            base_instructions,
            dynamic_tools,
            event_persistence_mode,
        }
    }

    pub fn resume(path: PathBuf, event_persistence_mode: EventPersistenceMode) -> Self {
        Self::Resume {
            path,
            event_persistence_mode,
        }
    }
}

const PERSISTED_EXEC_AGGREGATED_OUTPUT_MAX_BYTES: usize = 10_000;

fn sanitize_rollout_item_for_persistence(
    item: RolloutItem,
    mode: EventPersistenceMode,
) -> RolloutItem {
    if mode != EventPersistenceMode::Extended {
        return item;
    }

    match item {
        RolloutItem::EventMsg(EventMsg::ExecCommandEnd(mut event)) => {
            // Persist only a bounded aggregated summary of command output.
            event.aggregated_output = truncate_middle_chars(
                &event.aggregated_output,
                PERSISTED_EXEC_AGGREGATED_OUTPUT_MAX_BYTES,
            );
            // Drop unnecessary fields from rollout storage since aggregated_output is all we need.
            event.stdout.clear();
            event.stderr.clear();
            event.formatted_output.clear();
            RolloutItem::EventMsg(EventMsg::ExecCommandEnd(event))
        }
        _ => item,
    }
}

impl RolloutRecorder {
    /// Attempt to create a new [`RolloutRecorder`].
    ///
    /// For newly created sessions, this precomputes path/metadata and defers
    /// file creation/open until an explicit `persist()` call.
    ///
    /// For resumed sessions, this immediately opens the existing rollout file.
    pub async fn new(
        config: &impl RolloutConfigView,
        params: RolloutRecorderParams,
        state_db_ctx: Option<StateDbHandle>,
        state_builder: Option<ThreadMetadataBuilder>,
    ) -> std::io::Result<Self> {
        let praxis_home = config.praxis_home().to_path_buf();
        let (
            file,
            deferred_log_file_info,
            rollout_path,
            meta,
            event_persistence_mode,
            native_writer,
            native_init,
        ) = match params {
            RolloutRecorderParams::Create {
                conversation_id,
                forked_from_id,
                source,
                base_instructions,
                dynamic_tools,
                event_persistence_mode,
            } => {
                let log_file_info = precompute_log_file_info(config, conversation_id)?;
                let path = log_file_info.path.clone();
                let session_id = log_file_info.conversation_id;
                let started_at = log_file_info.timestamp;

                let timestamp_format: &[FormatItem] = format_description!(
                    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
                );
                let timestamp = started_at
                    .to_offset(time::UtcOffset::UTC)
                    .format(timestamp_format)
                    .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

                let native_init = NativeRolloutInit {
                    praxis_home: praxis_home.clone(),
                    thread_id: conversation_id,
                    source: source.to_string(),
                    workspace: config.cwd().to_string_lossy().into_owned(),
                };

                let session_meta = SessionMeta {
                    id: session_id,
                    forked_from_id,
                    timestamp,
                    cwd: config.cwd().to_path_buf(),
                    originator: originator().value,
                    cli_version: env!("CARGO_PKG_VERSION").to_string(),
                    agent_base_name: source.get_agent_base_name(),
                    agent_title: source.get_agent_title(),
                    agent_display_name: source.get_agent_display_name(),
                    agent_role: source.get_agent_role(),
                    agent_path: source.get_agent_path().map(Into::into),
                    source,
                    model_provider: Some(config.model_provider_id().to_string()),
                    base_instructions: Some(base_instructions),
                    dynamic_tools: if dynamic_tools.is_empty() {
                        None
                    } else {
                        Some(dynamic_tools)
                    },
                    memory_mode: (!config.generate_memories()).then_some("disabled".to_string()),
                };

                (
                    None,
                    Some(log_file_info),
                    path,
                    Some(session_meta),
                    event_persistence_mode,
                    None,
                    Some(native_init),
                )
            }
            RolloutRecorderParams::Resume {
                path,
                event_persistence_mode,
            } => {
                let native_writer = NativeRolloutWriter::resume(praxis_home, &path).await?;
                let file = tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .await?;
                (
                    Some(file),
                    None,
                    path,
                    None,
                    event_persistence_mode,
                    Some(native_writer),
                    None,
                )
            }
        };

        // Clone the cwd for the spawned task to collect git info asynchronously
        let cwd = config.cwd().to_path_buf();

        // A reasonably-sized bounded channel. If the buffer fills up the send
        // future will yield, which is fine – we only need to ensure we do not
        // perform *blocking* I/O on the caller's thread.
        let (tx, rx) = mpsc::channel::<RolloutCmd>(256);
        // Spawn a Tokio task that owns the file handle and performs async
        // writes. Using `tokio::fs::File` keeps everything on the async I/O
        // driver instead of blocking the runtime.
        tokio::task::spawn(rollout_writer(
            file,
            deferred_log_file_info,
            native_writer,
            native_init,
            rx,
            meta,
            cwd,
            rollout_path.clone(),
            state_db_ctx.clone(),
            state_builder,
            config.model_provider_id().to_string(),
            config.generate_memories(),
        ));

        Ok(Self {
            tx,
            rollout_path,
            state_db: state_db_ctx,
            event_persistence_mode,
        })
    }

    pub fn rollout_path(&self) -> &Path {
        self.rollout_path.as_path()
    }

    pub fn state_db(&self) -> Option<StateDbHandle> {
        self.state_db.clone()
    }

    pub async fn record_items(&self, items: &[RolloutItem]) -> std::io::Result<()> {
        let mut filtered = Vec::new();
        for item in items {
            // Note that function calls may look a bit strange if they are
            // "fully qualified MCP tool calls," so we could consider
            // reformatting them in that case.
            if is_persisted_response_item(item, self.event_persistence_mode) {
                filtered.push(sanitize_rollout_item_for_persistence(
                    item.clone(),
                    self.event_persistence_mode,
                ));
            }
        }
        if filtered.is_empty() {
            return Ok(());
        }
        self.tx
            .send(RolloutCmd::AddItems(filtered))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout items: {e}")))
    }

    /// Materialize the rollout file and persist all buffered items.
    ///
    /// This is idempotent; after first materialization, repeated calls are no-ops.
    pub async fn persist(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RolloutCmd::Persist { ack: tx })
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout persist: {e}")))?;
        rx.await
            .map_err(|e| IoError::other(format!("failed waiting for rollout persist: {e}")))?
    }

    pub async fn set_thread_name(&self, name: String) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RolloutCmd::SetThreadName { name, ack: tx })
            .await
            .map_err(|e| IoError::other(format!("failed to queue thread name: {e}")))?;
        rx.await
            .map_err(|e| IoError::other(format!("failed waiting for thread name: {e}")))?
    }

    /// Flush all queued writes and wait until they are committed by the writer task.
    pub async fn flush(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RolloutCmd::Flush { ack: tx })
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout flush: {e}")))?;
        rx.await
            .map_err(|e| IoError::other(format!("failed waiting for rollout flush: {e}")))?
    }

    pub async fn shutdown(&self) -> std::io::Result<()> {
        let (tx_done, rx_done) = oneshot::channel();
        match self.tx.send(RolloutCmd::Shutdown { ack: tx_done }).await {
            Ok(_) => rx_done.await.map_err(|e| {
                IoError::other(format!("failed waiting for rollout shutdown: {e}"))
            })??,
            Err(e) => {
                warn!("failed to send rollout shutdown command: {e}");
                return Err(IoError::other(format!(
                    "failed to send rollout shutdown command: {e}"
                )));
            }
        };
        Ok(())
    }
}

struct LogFileInfo {
    /// Full path to the rollout file.
    path: PathBuf,

    /// Session ID (also embedded in filename).
    conversation_id: ThreadId,

    /// Timestamp for the start of the session.
    timestamp: OffsetDateTime,
}

fn precompute_log_file_info(
    config: &impl RolloutConfigView,
    conversation_id: ThreadId,
) -> std::io::Result<LogFileInfo> {
    // Resolve ~/.praxis/sessions/YYYY/MM/DD path.
    let timestamp = OffsetDateTime::now_local()
        .map_err(|e| IoError::other(format!("failed to get local time: {e}")))?;
    let mut dir = config.praxis_home().to_path_buf();
    dir.push(SESSIONS_SUBDIR);
    dir.push(timestamp.year().to_string());
    dir.push(format!("{:02}", u8::from(timestamp.month())));
    dir.push(format!("{:02}", timestamp.day()));

    // Custom format for YYYY-MM-DDThh-mm-ss. Use `-` instead of `:` for
    // compatibility with filesystems that do not allow colons in filenames.
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let date_str = timestamp
        .format(format)
        .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

    let filename = format!("rollout-{date_str}-{conversation_id}.jsonl");

    let path = dir.join(filename);

    Ok(LogFileInfo {
        path,
        conversation_id,
        timestamp,
    })
}

fn open_log_file(path: &Path) -> std::io::Result<File> {
    let Some(parent) = path.parent() else {
        return Err(IoError::other(format!(
            "rollout path has no parent: {}",
            path.display()
        )));
    };
    fs::create_dir_all(parent)?;
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
}

#[allow(clippy::too_many_arguments)]
async fn rollout_writer(
    file: Option<tokio::fs::File>,
    mut deferred_log_file_info: Option<LogFileInfo>,
    mut native_writer: Option<NativeRolloutWriter>,
    mut native_init: Option<NativeRolloutInit>,
    mut rx: mpsc::Receiver<RolloutCmd>,
    mut meta: Option<SessionMeta>,
    cwd: std::path::PathBuf,
    rollout_path: PathBuf,
    state_db_ctx: Option<StateDbHandle>,
    mut state_builder: Option<ThreadMetadataBuilder>,
    default_provider: String,
    generate_memories: bool,
) -> std::io::Result<()> {
    let mut locator_writer = file.map(JsonlWriter::new);
    let mut persisted = locator_writer.is_some();
    let mut buffered_items = Vec::<RolloutItem>::new();
    let mut pending_name = None;
    let mut name_reconciled = false;
    if let Some(builder) = state_builder.as_mut() {
        builder.rollout_path = rollout_path.clone();
    }

    // Resumed sessions already have a file handle open, so session metadata can
    // be written immediately if present.
    if locator_writer.is_some()
        && let Some(session_meta) = meta.take()
    {
        write_session_meta(
            native_writer.as_mut(),
            locator_writer.as_mut(),
            session_meta,
            &cwd,
            &rollout_path,
            state_db_ctx.as_deref(),
            &mut state_builder,
            default_provider.as_str(),
            generate_memories,
        )
        .await?;
    }
    drop(locator_writer);

    // Process rollout commands
    while let Some(cmd) = rx.recv().await {
        match cmd {
            RolloutCmd::AddItems(items) => {
                if items.is_empty() {
                    continue;
                }

                if !persisted {
                    buffered_items.extend(items);
                    continue;
                }

                if !name_reconciled {
                    reconcile_thread_name_from_state(
                        native_writer.as_ref(),
                        state_db_ctx.as_deref(),
                        &rollout_path,
                    )
                    .await?;
                    name_reconciled = true;
                }

                write_native_items_and_project_state(
                    native_writer.as_mut(),
                    items.as_slice(),
                    &rollout_path,
                    state_db_ctx.as_deref(),
                    state_builder.as_ref(),
                    default_provider.as_str(),
                )
                .await?;
            }
            RolloutCmd::SetThreadName { name, ack } => {
                let result = async {
                    if let Some(native_writer) = native_writer.as_ref() {
                        native_writer.set_name(name.clone()).await?;
                    } else {
                        pending_name = Some(name.clone());
                    }
                    if let Some(state_db) = state_db_ctx.as_deref() {
                        if let Err(error) = state_db
                            .set_thread_name(
                                crate::thread_store::thread_id_from_rollout_path(&rollout_path)
                                    .ok_or_else(|| {
                                        IoError::other("rollout path does not contain a thread id")
                                    })?,
                                &name,
                            )
                            .await
                        {
                            warn!("failed to update compatibility thread name projection: {error}");
                        }
                    }
                    Ok(())
                }
                .await;
                acknowledge(ack, result)?;
            }
            RolloutCmd::Persist { ack } => {
                let result = if !persisted {
                    async {
                        let Some(log_file_info) = deferred_log_file_info.take() else {
                            return Err(IoError::other(
                                "deferred rollout recorder missing log file metadata",
                            ));
                        };
                        let init = native_init.take().ok_or_else(|| {
                            IoError::other("deferred rollout recorder missing native metadata")
                        })?;
                        native_writer = Some(
                            NativeRolloutWriter::open(init, log_file_info.path.as_path()).await?,
                        );
                        let file = open_log_file(log_file_info.path.as_path())?;
                        let mut locator_writer = JsonlWriter::new(tokio::fs::File::from_std(file));

                        if let Some(session_meta) = meta.take() {
                            write_session_meta(
                                native_writer.as_mut(),
                                Some(&mut locator_writer),
                                session_meta,
                                &cwd,
                                &rollout_path,
                                state_db_ctx.as_deref(),
                                &mut state_builder,
                                default_provider.as_str(),
                                generate_memories,
                            )
                            .await?;
                        }

                        if let Some(name) = pending_name.take() {
                            native_writer
                                .as_ref()
                                .ok_or_else(|| {
                                    IoError::other("rollout recorder missing native thread writer")
                                })?
                                .set_name(name)
                                .await?;
                            name_reconciled = true;
                        } else if !name_reconciled {
                            reconcile_thread_name_from_state(
                                native_writer.as_ref(),
                                state_db_ctx.as_deref(),
                                &rollout_path,
                            )
                            .await?;
                            name_reconciled = true;
                        }

                        if !buffered_items.is_empty() {
                            write_native_items_and_project_state(
                                native_writer.as_mut(),
                                buffered_items.as_slice(),
                                &rollout_path,
                                state_db_ctx.as_deref(),
                                state_builder.as_ref(),
                                default_provider.as_str(),
                            )
                            .await?;
                            buffered_items.clear();
                        }
                        locator_writer.flush().await?;
                        persisted = true;
                        flush_writers(native_writer.as_ref()).await?;

                        Ok(())
                    }
                    .await
                } else {
                    flush_writers(native_writer.as_ref()).await
                };
                acknowledge(ack, result)?;
            }
            RolloutCmd::Flush { ack } => {
                // Deferred fresh threads may not have an initialized file yet.
                let result = flush_writers(native_writer.as_ref()).await;
                acknowledge(ack, result)?;
            }
            RolloutCmd::Shutdown { ack } => {
                let result = flush_writers(native_writer.as_ref()).await;
                acknowledge(ack, result)?;
                break;
            }
        }
    }

    flush_writers(native_writer.as_ref()).await?;
    Ok(())
}

fn acknowledge(
    ack: oneshot::Sender<std::io::Result<()>>,
    result: std::io::Result<()>,
) -> std::io::Result<()> {
    match result {
        Ok(()) => {
            let _ = ack.send(Ok(()));
            Ok(())
        }
        Err(error) => {
            let writer_error = IoError::new(error.kind(), error.to_string());
            let _ = ack.send(Err(error));
            Err(writer_error)
        }
    }
}

async fn flush_writers(native_writer: Option<&NativeRolloutWriter>) -> std::io::Result<()> {
    if let Some(native_writer) = native_writer {
        native_writer.sync().await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn write_session_meta(
    native_writer: Option<&mut NativeRolloutWriter>,
    mut writer: Option<&mut JsonlWriter>,
    session_meta: SessionMeta,
    cwd: &Path,
    rollout_path: &Path,
    state_db_ctx: Option<&StateRuntime>,
    state_builder: &mut Option<ThreadMetadataBuilder>,
    default_provider: &str,
    generate_memories: bool,
) -> std::io::Result<()> {
    let git_info = collect_git_info(cwd).await.map(|info| ProtocolGitInfo {
        commit_hash: info.commit_hash,
        branch: info.branch,
        repository_url: info.repository_url,
    });
    let session_meta_line = SessionMetaLine {
        meta: session_meta,
        git: git_info,
    };
    if state_db_ctx.is_some() {
        *state_builder = metadata::builder_from_session_meta(&session_meta_line, rollout_path);
    }

    let rollout_item = RolloutItem::SessionMeta(session_meta_line);
    let native_writer = native_writer
        .ok_or_else(|| IoError::other("rollout recorder missing native thread writer"))?;
    native_writer
        .append(std::slice::from_ref(&rollout_item))
        .await?;
    if let Some(writer) = writer.as_mut() {
        writer.write_rollout_item(&rollout_item).await?;
        writer.flush().await?;
    }
    sync_thread_state_after_write(
        state_db_ctx,
        rollout_path,
        state_builder.as_ref(),
        std::slice::from_ref(&rollout_item),
        default_provider,
        (!generate_memories).then_some("disabled"),
    )
    .await;
    Ok(())
}

async fn write_native_items_and_project_state(
    native_writer: Option<&mut NativeRolloutWriter>,
    items: &[RolloutItem],
    rollout_path: &Path,
    state_db_ctx: Option<&StateRuntime>,
    state_builder: Option<&ThreadMetadataBuilder>,
    default_provider: &str,
) -> std::io::Result<()> {
    let native_writer = native_writer
        .ok_or_else(|| IoError::other("rollout recorder missing native thread writer"))?;
    native_writer.append(items).await?;
    sync_thread_state_after_write(
        state_db_ctx,
        rollout_path,
        state_builder,
        items,
        default_provider,
        /*new_thread_memory_mode*/ None,
    )
    .await;
    Ok(())
}

async fn reconcile_thread_name_from_state(
    native_writer: Option<&NativeRolloutWriter>,
    state_db: Option<&praxis_state::StateRuntime>,
    rollout_path: &Path,
) -> std::io::Result<()> {
    let (Some(native_writer), Some(state_db), Some(thread_id)) = (
        native_writer,
        state_db,
        crate::thread_store::thread_id_from_rollout_path(rollout_path),
    ) else {
        return Ok(());
    };
    let names = match state_db
        .get_thread_names(&HashSet::from([thread_id]))
        .await
    {
        Ok(names) => names,
        Err(error) => {
            warn!("failed to read compatibility thread name projection: {error}");
            return Ok(());
        }
    };
    if let Some(name) = names.get(&thread_id) {
        native_writer.set_name(name.clone()).await?;
    }
    Ok(())
}

async fn sync_thread_state_after_write(
    state_db_ctx: Option<&StateRuntime>,
    rollout_path: &Path,
    state_builder: Option<&ThreadMetadataBuilder>,
    items: &[RolloutItem],
    default_provider: &str,
    new_thread_memory_mode: Option<&str>,
) {
    let updated_at = Utc::now();
    if new_thread_memory_mode.is_some()
        || items
            .iter()
            .any(praxis_state::rollout_item_affects_thread_metadata)
    {
        state_db::apply_rollout_items(
            state_db_ctx,
            rollout_path,
            default_provider,
            state_builder,
            items,
            "rollout_writer",
            new_thread_memory_mode,
            Some(updated_at),
        )
        .await;
        return;
    }

    let thread_id = state_builder
        .map(|builder| builder.id)
        .or_else(|| metadata::builder_from_items(items, rollout_path).map(|builder| builder.id));
    if state_db::touch_thread_updated_at(state_db_ctx, thread_id, updated_at, "rollout_writer")
        .await
    {
        return;
    }
    state_db::apply_rollout_items(
        state_db_ctx,
        rollout_path,
        default_provider,
        state_builder,
        items,
        "rollout_writer",
        new_thread_memory_mode,
        Some(updated_at),
    )
    .await;
}

#[cfg(test)]
#[path = "recorder_tests.rs"]
mod tests;

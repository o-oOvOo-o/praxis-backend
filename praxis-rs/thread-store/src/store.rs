use crate::LiveThreadStore;
use crate::ThreadListPage;
use crate::ThreadListQuery;
use crate::ThreadStoreError;
use crate::index::ThreadIndex;
use crate::live_thread::ThreadSessionMetadataState;
use crate::projection::ThreadIndexAccumulator;
use crate::projection::ThreadIndexProjection;
use crate::projection::TranscriptScanPlan;
use praxis_thread_store_contracts::ContentRef;
use praxis_thread_store_contracts::ThreadActor;
use praxis_thread_store_contracts::ThreadCommand;
use praxis_thread_store_contracts::ThreadEventBody;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadId;
use praxis_thread_store_contracts::ThreadRevision;
use praxis_thread_store_contracts::ThreadRevisionRef;
use praxis_thread_store_journal::JournalConfig;
use praxis_thread_store_journal::ThreadJournal;
use praxis_thread_store_journal::ThreadRevisionRange;
use praxis_thread_store_journal::consume_snapshot;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc;

const FORK_REPLAY_BUFFER: usize = 16;

#[derive(Clone, Debug)]
pub struct ThreadStore {
    root: Arc<PathBuf>,
    index: ThreadIndex,
}

/// Describes whether a recovery fold contains complete or checkpointed model context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelContextFoldCoverage {
    Complete,
    Checkpointed,
    CheckpointInvalidated,
}

/// Borrowed restore checkpoints from the current native thread projection.
#[derive(Clone, Copy)]
pub struct ThreadOpenIndex<'a> {
    projection: Option<&'a ThreadIndexProjection>,
}

/// Store-owned projection reserved for one native lifecycle operation.
pub struct PreparedThreadProjection {
    store_root: Arc<PathBuf>,
    thread_id: ThreadId,
    projection: ThreadIndexProjection,
}

/// Result metadata from one immutable prepared journal fold.
pub struct PreparedThreadFold {
    summary: crate::ThreadSummary,
    model_context_coverage: ModelContextFoldCoverage,
}

pub struct RecoveredThreadState {
    summary: crate::ThreadSummary,
    next_agent_event_sequence: u64,
}

impl RecoveredThreadState {
    pub fn summary(&self) -> &crate::ThreadSummary {
        &self.summary
    }

    pub fn next_agent_event_sequence(&self) -> u64 {
        self.next_agent_event_sequence
    }

    pub fn into_summary(self) -> crate::ThreadSummary {
        self.summary
    }
}

impl PreparedThreadProjection {
    pub fn summary(&self) -> &crate::ThreadSummary {
        &self.projection.summary
    }

    pub fn open_index(&self) -> ThreadOpenIndex<'_> {
        ThreadOpenIndex::new(Some(&self.projection))
    }
}

impl PreparedThreadFold {
    pub fn model_context_coverage(&self) -> ModelContextFoldCoverage {
        self.model_context_coverage
    }

    pub fn into_summary(self) -> crate::ThreadSummary {
        self.summary
    }
}

impl<'a> ThreadOpenIndex<'a> {
    fn new(projection: Option<&'a ThreadIndexProjection>) -> Self {
        Self { projection }
    }

    pub fn model_context_checkpoint(self) -> Option<ThreadRevision> {
        self.projection
            .and_then(|projection| projection.model_context_checkpoint)
    }

    pub fn dynamic_tools_match(self, tools: &ContentRef) -> bool {
        self.projection
            .is_some_and(|projection| projection.dynamic_tools_match(tools))
    }

    pub fn transcript_total_turns(self) -> Option<usize> {
        self.projection
            .map(|projection| projection.transcript_index.total_turns())
    }

    pub fn transcript_plan_from(self, first_turn: usize) -> Option<TranscriptScanPlan> {
        self.projection
            .and_then(|projection| projection.transcript_index.plan_from(first_turn))
    }
}

impl ModelContextFoldCoverage {
    /// Returns whether the consumer must rebuild model context from the complete journal.
    pub const fn requires_complete_replay(self) -> bool {
        matches!(self, Self::CheckpointInvalidated)
    }
}

fn model_context_fold_coverage(
    opened: Option<ThreadRevision>,
    recovered: Option<ThreadRevision>,
) -> ModelContextFoldCoverage {
    match opened {
        None => ModelContextFoldCoverage::Complete,
        Some(opened) if recovered.is_some_and(|current| current >= opened) => {
            ModelContextFoldCoverage::Checkpointed
        }
        Some(_) => ModelContextFoldCoverage::CheckpointInvalidated,
    }
}

impl ThreadStore {
    pub fn from_praxis_home(praxis_home: impl AsRef<Path>) -> Self {
        Self::new(Self::root_path(praxis_home))
    }

    pub fn root_path(praxis_home: impl AsRef<Path>) -> PathBuf {
        praxis_home.as_ref().join(crate::THREAD_STORE_SUBDIR)
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            index: ThreadIndex::new(&root),
            root: Arc::new(root),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn writer_is_busy(&self, thread_id: ThreadId) -> Result<bool, ThreadStoreError> {
        let config = JournalConfig::new(self.root.as_ref().clone());
        Ok(ThreadJournal::writer_is_busy(&config, thread_id)?)
    }

    /// Return whether the canonical journal directory exists without blocking the caller.
    pub async fn thread_exists(&self, thread_id: ThreadId) -> bool {
        tokio::fs::metadata(self.journal_path(thread_id))
            .await
            .is_ok_and(|metadata| metadata.is_dir())
    }

    pub fn journal_path(&self, thread_id: ThreadId) -> PathBuf {
        self.root
            .join("threads")
            .join(thread_id.to_string())
            .join("journal")
    }

    pub async fn open_thread(
        &self,
        thread_id: ThreadId,
    ) -> Result<LiveThreadStore, ThreadStoreError> {
        self.open_thread_and_fold(thread_id, |_| (), |_, _| {})
            .await
            .map(|(thread, (), _)| thread)
    }

    /// Open the writer and initialize a fold from the borrowed store-owned restore index.
    pub async fn open_thread_and_fold<S, F, I>(
        &self,
        thread_id: ThreadId,
        initialize: I,
        fold: F,
    ) -> Result<(LiveThreadStore, S, ModelContextFoldCoverage), ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, ThreadEventEnvelope) + Send + 'static,
        I: for<'a> FnOnce(ThreadOpenIndex<'a>) -> S + Send,
    {
        let indexed = self.index.read_projection(thread_id).await?;
        self.open_thread_and_fold_with_projection(thread_id, indexed, initialize, fold)
            .await
            .map(|(thread, state, coverage, _)| (thread, state, coverage))
    }

    /// Read and own the complete restore projection before deriving runtime configuration.
    pub async fn prepare_thread_projection(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<PreparedThreadProjection>, ThreadStoreError> {
        if !self.thread_exists(thread_id).await {
            return Ok(None);
        }
        let projection = match self.index.read_projection(thread_id).await? {
            Some(projection) => projection,
            None => match self.rebuild_projection(thread_id).await? {
                Some(projection) => projection,
                None => return Ok(None),
            },
        };
        Ok(Some(PreparedThreadProjection {
            store_root: Arc::clone(&self.root),
            thread_id,
            projection,
        }))
    }

    /// Consume a prepared projection while folding one immutable journal snapshot.
    pub async fn fold_prepared_thread_events<S, F>(
        &self,
        prepared: PreparedThreadProjection,
        state: S,
        mut fold: F,
    ) -> Result<(S, PreparedThreadFold), ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &ThreadEventEnvelope) + Send + 'static,
    {
        self.ensure_prepared_root(&prepared.store_root)?;
        let prepared_revision = prepared.projection.summary.revision;
        let prepared_checkpoint = prepared.projection.model_context_checkpoint;
        let folded = self
            .fold_existing_thread_events(
                prepared.thread_id,
                PreparedEventFold {
                    state,
                    index: ThreadIndexAccumulator::from_projection(prepared.projection),
                    last_seen: ThreadRevision::ZERO,
                },
                move |folded, event| {
                    folded.last_seen = event.revision;
                    folded.index.push(event);
                    fold(&mut folded.state, event);
                },
            )
            .await?;
        let PreparedEventFold {
            state,
            index,
            last_seen,
        } = folded;
        if last_seen < prepared_revision {
            return Err(ThreadStoreError::PreparedProjectionAhead {
                prepared: prepared_revision.get(),
                recovered: last_seen.get(),
            });
        }
        let projection = index.finish().ok_or(ThreadStoreError::ThreadNotCreated)?;
        if projection.summary.revision != prepared_revision {
            self.index.synchronize(&projection).await?;
        }
        let model_context_coverage =
            model_context_fold_coverage(prepared_checkpoint, projection.model_context_checkpoint);
        Ok((
            state,
            PreparedThreadFold {
                summary: projection.summary,
                model_context_coverage,
            },
        ))
    }

    /// Recover from a previously prepared projection without another index read.
    pub async fn open_prepared_thread_and_fold<S, F, I>(
        &self,
        prepared: PreparedThreadProjection,
        initialize: I,
        fold: F,
    ) -> Result<
        (
            LiveThreadStore,
            S,
            ModelContextFoldCoverage,
            RecoveredThreadState,
        ),
        ThreadStoreError,
    >
    where
        S: Send + 'static,
        F: FnMut(&mut S, ThreadEventEnvelope) + Send + 'static,
        I: for<'a> FnOnce(ThreadOpenIndex<'a>) -> S + Send,
    {
        self.ensure_prepared_root(&prepared.store_root)?;
        let (thread, state, coverage, summary) = self
            .open_thread_and_fold_with_projection(
                prepared.thread_id,
                Some(prepared.projection),
                initialize,
                fold,
            )
            .await?;
        Ok((
            thread,
            state,
            coverage,
            summary.ok_or(ThreadStoreError::ThreadNotCreated)?,
        ))
    }

    async fn open_thread_and_fold_with_projection<S, F, I>(
        &self,
        thread_id: ThreadId,
        indexed: Option<ThreadIndexProjection>,
        initialize: I,
        mut fold: F,
    ) -> Result<
        (
            LiveThreadStore,
            S,
            ModelContextFoldCoverage,
            Option<RecoveredThreadState>,
        ),
        ThreadStoreError,
    >
    where
        S: Send + 'static,
        F: FnMut(&mut S, ThreadEventEnvelope) + Send + 'static,
        I: for<'a> FnOnce(ThreadOpenIndex<'a>) -> S + Send,
    {
        let indexed_revision = indexed
            .as_ref()
            .map(|projection| projection.summary.revision);
        let open_index = ThreadOpenIndex::new(indexed.as_ref());
        let opened_checkpoint = open_index.model_context_checkpoint();
        let state = initialize(open_index);
        let index = indexed.map_or_else(
            ThreadIndexAccumulator::default,
            ThreadIndexAccumulator::from_projection,
        );
        let config = JournalConfig::new(self.root.as_ref().clone());
        let recovery = tokio::task::spawn_blocking(move || {
            let (journal, mut recovered) = ThreadJournal::open_and_fold(
                config,
                thread_id,
                OpenThreadFold { index, state },
                move |recovered, event| {
                    recovered.index.push(&event);
                    fold(&mut recovered.state, event);
                },
            )?;
            if recovered
                .index
                .revision()
                .is_some_and(|revision| revision > journal.head().revision)
            {
                recovered.index = rebuild_index(&journal)?;
            }
            Ok::<_, praxis_thread_store_journal::JournalError>((journal, recovered))
        });
        let recovery = recovery
            .await
            .map_err(|error| ThreadStoreError::Worker(error.to_string()))?;
        let (journal, recovered) = recovery?;
        let recovered_index = recovered.index.finish();
        let recovered_checkpoint = recovered_index
            .as_ref()
            .and_then(|projection| projection.model_context_checkpoint);
        let coverage = model_context_fold_coverage(opened_checkpoint, recovered_checkpoint);
        match recovered_index.as_ref() {
            Some(projection) if indexed_revision != Some(projection.summary.revision) => {
                self.index.synchronize(projection).await?;
            }
            None if indexed_revision.is_some() => self.index.remove(thread_id).await?,
            Some(_) | None => {}
        }
        let session_metadata = recovered_index.as_ref().map_or_else(
            || ThreadSessionMetadataState::new(None, None, None, None, None),
            |projection| {
                ThreadSessionMetadataState::new(
                    projection.summary.name.clone(),
                    projection.summary.model.clone(),
                    projection.summary.model_provider.clone(),
                    projection.summary.reasoning_effort.clone(),
                    projection.dynamic_tools_digest,
                )
            },
        );
        let thread = LiveThreadStore::new(
            thread_id,
            Arc::new(Mutex::new(journal)),
            self.index.clone(),
            session_metadata,
        );
        let summary = recovered_index.map(|projection| RecoveredThreadState {
            summary: projection.summary,
            next_agent_event_sequence: projection.last_agent_sequence.saturating_add(1).max(1),
        });
        Ok((thread, recovered.state, coverage, summary))
    }

    fn ensure_prepared_root(&self, root: &Path) -> Result<(), ThreadStoreError> {
        if root == self.root.as_path() {
            return Ok(());
        }
        Err(ThreadStoreError::Worker(
            "prepared thread belongs to a different ThreadStore".to_string(),
        ))
    }

    pub async fn list_threads(
        &self,
        query: ThreadListQuery,
    ) -> Result<ThreadListPage, ThreadStoreError> {
        self.index.initialize().await?;
        self.index.list(query).await
    }

    /// Fold an immutable journal snapshot without allocating an event projection.
    pub async fn fold_thread_events<S, F>(
        &self,
        thread_id: ThreadId,
        state: S,
        fold: F,
    ) -> Result<Option<S>, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &praxis_thread_store_contracts::ThreadEventEnvelope) + Send + 'static,
    {
        if !self.thread_exists(thread_id).await {
            return Ok(None);
        }
        self.fold_existing_thread_events(thread_id, state, fold)
            .await
            .map(Some)
    }

    async fn fold_existing_thread_events<S, F>(
        &self,
        thread_id: ThreadId,
        state: S,
        mut fold: F,
    ) -> Result<S, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &praxis_thread_store_contracts::ThreadEventEnvelope) + Send + 'static,
    {
        self.consume_existing_thread_events(thread_id, state, move |state, event| {
            fold(state, &event)
        })
        .await
    }

    async fn consume_existing_thread_events<S, F>(
        &self,
        thread_id: ThreadId,
        state: S,
        consume: F,
    ) -> Result<S, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, ThreadEventEnvelope) + Send + 'static,
    {
        let config = JournalConfig::new(self.root.as_ref().clone());
        tokio::task::spawn_blocking(move || {
            Ok::<_, ThreadStoreError>(consume_snapshot(config, thread_id, state, consume)?)
        })
        .await
        .map_err(|error| ThreadStoreError::Worker(error.to_string()))?
    }

    /// Read the indexed thread summary without hydrating transcript events.
    pub async fn read_summary(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<crate::ThreadSummary>, ThreadStoreError> {
        if !self.thread_exists(thread_id).await {
            return Ok(None);
        }
        if let Some(summary) = self.index.read_summary(thread_id).await? {
            return Ok(Some(summary));
        }
        let projection = self.rebuild_projection(thread_id).await?;
        Ok(projection.map(|projection| projection.summary))
    }

    async fn rebuild_projection(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadIndexProjection>, ThreadStoreError> {
        let accumulator = self
            .fold_existing_thread_events(
                thread_id,
                ThreadIndexAccumulator::default(),
                ThreadIndexAccumulator::push,
            )
            .await?;
        let Some(projection) = accumulator.finish() else {
            return Ok(None);
        };
        self.index.replace(&projection).await?;
        Ok(Some(projection))
    }

    pub async fn set_archived(
        &self,
        thread_id: ThreadId,
        archived: bool,
    ) -> Result<(), ThreadStoreError> {
        let thread = self.open_thread(thread_id).await?;
        thread
            .execute(
                ThreadActor::User,
                None,
                ThreadCommand::SetArchived { archived },
                crate::CommitMode::Durable,
            )
            .await?;
        Ok(())
    }

    /// Permanently remove one exact thread journal and its rebuildable projection.
    pub async fn delete_thread(&self, thread_id: ThreadId) -> Result<bool, ThreadStoreError> {
        let thread_dir = self.root.join("threads").join(thread_id.to_string());
        if !self.thread_exists(thread_id).await {
            return Ok(false);
        }
        let expected_parent = self.root.join("threads");
        if thread_dir.parent() != Some(expected_parent.as_path()) {
            return Err(ThreadStoreError::Worker(
                "refusing to delete a thread outside the canonical store".to_string(),
            ));
        }
        self.index.remove(thread_id).await?;
        tokio::fs::remove_dir_all(thread_dir)
            .await
            .map_err(|error| ThreadStoreError::Worker(error.to_string()))?;
        Ok(true)
    }

    pub async fn rollback(
        &self,
        thread_id: ThreadId,
        to_revision: ThreadRevision,
        reason: impl Into<String>,
    ) -> Result<(), ThreadStoreError> {
        let thread = self.open_thread(thread_id).await?;
        let from_revision = thread.head().await?.revision;
        if to_revision > from_revision {
            return Err(ThreadStoreError::RevisionNotFound(to_revision.get()));
        }
        thread
            .execute(
                ThreadActor::User,
                None,
                ThreadCommand::MoveTranscriptHead {
                    from_revision,
                    to_revision,
                    reason: reason.into(),
                },
                crate::CommitMode::Durable,
            )
            .await?;
        Ok(())
    }

    /// Fork canonical facts while allowing the owning product to rebind embedded payloads.
    pub async fn fork_thread<F>(
        &self,
        source_id: ThreadId,
        through_revision: ThreadRevision,
        target_id: ThreadId,
        mut transform: F,
    ) -> Result<LiveThreadStore, ThreadStoreError>
    where
        F: FnMut(ThreadCommand) -> Result<Option<ThreadCommand>, ThreadStoreError>,
    {
        if !self.thread_exists(source_id).await {
            return Err(ThreadStoreError::ThreadNotCreated);
        }
        let (sender, receiver) = mpsc::channel(FORK_REPLAY_BUFFER);
        let producer = async {
            self.consume_existing_thread_events(
                source_id,
                ForkReplayStream::new(through_revision, sender),
                ForkReplayStream::push,
            )
            .await?
            .complete()
            .await
        };
        let consumer = self.replay_fork_records(
            source_id,
            through_revision,
            target_id,
            receiver,
            &mut transform,
        );
        let (source_result, target_result) = tokio::join!(producer, consumer);
        if let Err(error) = source_result {
            if let Ok(target) = target_result {
                drop(target);
                let _ = self.delete_thread(target_id).await;
            }
            return Err(error);
        }
        target_result
    }

    async fn replay_fork_records<F>(
        &self,
        source_id: ThreadId,
        through_revision: ThreadRevision,
        target_id: ThreadId,
        mut receiver: mpsc::Receiver<ForkReplayRecord>,
        transform: &mut F,
    ) -> Result<LiveThreadStore, ThreadStoreError>
    where
        F: FnMut(ThreadCommand) -> Result<Option<ThreadCommand>, ThreadStoreError>,
    {
        let Some(ForkReplayRecord::Created { source, workspace }) = receiver.recv().await else {
            return Err(ThreadStoreError::Worker(
                "fork source ended before its creation fact".to_string(),
            ));
        };
        let target = self.open_thread(target_id).await?;
        if target.head().await? != praxis_thread_store_contracts::ThreadHead::EMPTY {
            return Err(ThreadStoreError::ThreadAlreadyExists);
        }
        let replay = async {
            let receipt = target
                .ensure_created(
                    source,
                    workspace,
                    Some(ThreadRevisionRef {
                        thread_id: source_id,
                        revision: through_revision,
                    }),
                )
                .await?;
            let mut target_revision = receipt.revision_after;
            loop {
                match receiver.recv().await {
                    Some(ForkReplayRecord::Command { revision, command }) => {
                        let command = rebase_fork_command(target_revision, command);
                        let Some(command) = transform(command)? else {
                            continue;
                        };
                        let receipt = target
                            .execute(
                                ThreadActor::System,
                                Some(format!("fork:{}:{}", source_id, revision.get())),
                                command,
                                crate::CommitMode::Buffered,
                            )
                            .await?;
                        target_revision = receipt.revision_after;
                    }
                    Some(ForkReplayRecord::Complete) => {
                        target.sync().await?;
                        return Ok::<(), ThreadStoreError>(());
                    }
                    Some(ForkReplayRecord::Created { .. }) => {
                        return Err(ThreadStoreError::Worker(
                            "fork source contains duplicate creation facts".to_string(),
                        ));
                    }
                    None => {
                        return Err(ThreadStoreError::Worker(
                            "fork source ended before validation completed".to_string(),
                        ));
                    }
                }
            }
        }
        .await;
        if let Err(error) = replay {
            drop(target);
            let _ = self.delete_thread(target_id).await;
            return Err(error);
        }
        Ok(target)
    }
}

fn rebuild_index(
    journal: &ThreadJournal,
) -> Result<ThreadIndexAccumulator, praxis_thread_store_journal::JournalError> {
    let head = journal.head().revision;
    if head == ThreadRevision::ZERO {
        return Ok(ThreadIndexAccumulator::default());
    }
    journal.fold_range(
        ThreadRevisionRange::inclusive(ThreadRevision::new(1), head)?,
        ThreadIndexAccumulator::default(),
        ThreadIndexAccumulator::push,
    )
}

struct OpenThreadFold<S> {
    index: ThreadIndexAccumulator,
    state: S,
}

struct PreparedEventFold<S> {
    state: S,
    index: ThreadIndexAccumulator,
    last_seen: ThreadRevision,
}

enum ForkReplayRecord {
    Created {
        source: String,
        workspace: String,
    },
    Command {
        revision: ThreadRevision,
        command: ThreadCommand,
    },
    Complete,
}

struct ForkReplayStream {
    through: ThreadRevision,
    sender: mpsc::Sender<ForkReplayRecord>,
    created: bool,
    reached_through: bool,
    receiver_closed: bool,
}

impl ForkReplayStream {
    fn new(through: ThreadRevision, sender: mpsc::Sender<ForkReplayRecord>) -> Self {
        Self {
            through,
            sender,
            created: false,
            reached_through: false,
            receiver_closed: false,
        }
    }

    fn push(&mut self, event: ThreadEventEnvelope) {
        if event.revision > self.through {
            return;
        }
        let revision = event.revision;
        let record = match (revision == ThreadRevision::new(1), event.body) {
            (
                true,
                ThreadEventBody::ThreadCreated {
                    source, workspace, ..
                },
            ) => {
                self.created = true;
                Some(ForkReplayRecord::Created { source, workspace })
            }
            (_, body) => {
                replay_command(body).map(|command| ForkReplayRecord::Command { revision, command })
            }
        };
        if !self.receiver_closed
            && let Some(record) = record
            && self.sender.blocking_send(record).is_err()
        {
            self.receiver_closed = true;
        }
        self.reached_through |= revision == self.through;
    }

    async fn complete(self) -> Result<(), ThreadStoreError> {
        if !self.reached_through {
            return Err(ThreadStoreError::RevisionNotFound(self.through.get()));
        }
        if !self.created {
            return Err(ThreadStoreError::ThreadNotCreated);
        }
        let _ = self.sender.send(ForkReplayRecord::Complete).await;
        Ok(())
    }
}

fn rebase_fork_command(target_revision: ThreadRevision, command: ThreadCommand) -> ThreadCommand {
    match command {
        ThreadCommand::ReplaceModelContextBaseline {
            summary,
            retained_item_ids,
            ..
        } => ThreadCommand::ReplaceModelContextBaseline {
            basis_revision: target_revision,
            summary,
            retained_item_ids,
        },
        ThreadCommand::ReplaceModelContextSnapshot { snapshot, .. } => {
            ThreadCommand::ReplaceModelContextSnapshot {
                basis_revision: target_revision,
                snapshot,
            }
        }
        command => command,
    }
}

fn replay_command(body: ThreadEventBody) -> Option<ThreadCommand> {
    Some(match body {
        ThreadEventBody::ThreadCreated { .. }
        | ThreadEventBody::ThreadArchived { .. }
        | ThreadEventBody::TranscriptHeadMoved { .. }
        | ThreadEventBody::ExternalHistoryImported { .. } => return None,
        ThreadEventBody::ThreadNameSet { name } => ThreadCommand::SetName { name },
        ThreadEventBody::ThreadSummarySet { summary } => ThreadCommand::SetSummary { summary },
        ThreadEventBody::ThreadWorkspaceSet { workspace } => {
            ThreadCommand::SetWorkspace { workspace }
        }
        ThreadEventBody::TurnStarted {
            turn_id,
            collaboration_mode,
        } => ThreadCommand::StartTurn {
            turn_id,
            collaboration_mode,
        },
        ThreadEventBody::TurnExecutionContextCaptured { turn_id, context } => {
            ThreadCommand::CaptureTurnExecutionContext { turn_id, context }
        }
        ThreadEventBody::NativeAgentEventRecorded {
            agent_sequence,
            event_id,
            turn_id,
            route,
            payload,
        } => ThreadCommand::RecordNativeAgentEvent {
            agent_sequence,
            event_id,
            turn_id,
            route,
            payload,
        },
        ThreadEventBody::TranscriptItemCreated {
            item_id,
            turn_id,
            item_kind,
            content,
        } => ThreadCommand::AppendTranscriptItem {
            item_id,
            turn_id,
            item_kind,
            content,
        },
        ThreadEventBody::TranscriptItemFinalized { item_id, content } => {
            ThreadCommand::FinalizeTranscriptItem { item_id, content }
        }
        ThreadEventBody::TranscriptItemCancelled { item_id, reason } => {
            ThreadCommand::CancelTranscriptItem { item_id, reason }
        }
        ThreadEventBody::TurnCompleted { turn_id } => ThreadCommand::CompleteTurn { turn_id },
        ThreadEventBody::TurnAborted { turn_id, reason } => {
            ThreadCommand::AbortTurn { turn_id, reason }
        }
        ThreadEventBody::TurnFailed {
            turn_id,
            error_code,
            message,
        } => ThreadCommand::FailTurn {
            turn_id,
            error_code,
            message,
        },
        ThreadEventBody::ModelContextBaselineReplaced {
            basis_revision,
            summary,
            retained_item_ids,
        } => ThreadCommand::ReplaceModelContextBaseline {
            basis_revision,
            summary,
            retained_item_ids,
        },
        ThreadEventBody::ModelContextSnapshotReplaced {
            basis_revision,
            snapshot,
        } => ThreadCommand::ReplaceModelContextSnapshot {
            basis_revision,
            snapshot,
        },
        ThreadEventBody::ModelContextRolledBack { user_turns } => {
            ThreadCommand::RollbackModelContext { user_turns }
        }
        ThreadEventBody::ContentRedacted {
            item_id,
            replacement,
            reason,
        } => ThreadCommand::RedactContent {
            item_id,
            replacement,
            reason,
        },
        ThreadEventBody::OpaqueImportedEvent {
            original_type,
            payload,
            ..
        } => ThreadCommand::AppendTranscriptItem {
            item_id: praxis_thread_store_contracts::ItemId::new(),
            turn_id: None,
            item_kind: praxis_thread_store_contracts::TranscriptItemKind::OpaqueImported,
            content: praxis_thread_store_contracts::ContentRef::InlineText {
                text: serde_json::json!({
                    "original_type": original_type,
                    "payload": payload,
                })
                .to_string(),
            },
        },
        ThreadEventBody::ModelContextItemRecorded {
            item_id,
            turn_id,
            content,
        } => ThreadCommand::RecordModelContextItem {
            item_id,
            turn_id,
            content,
        },
        ThreadEventBody::TurnCostRecorded { cost_micros } => {
            ThreadCommand::RecordTurnCost { cost_micros }
        }
        ThreadEventBody::ThreadResumeConfigSet {
            model,
            model_provider,
            reasoning_effort,
        } => ThreadCommand::SetResumeConfig {
            model,
            model_provider,
            reasoning_effort,
        },
        ThreadEventBody::ThreadDynamicToolsSet { tools } => {
            ThreadCommand::SetDynamicTools { tools }
        }
        ThreadEventBody::ThreadPreviewSet {
            preview,
            first_user_message,
        } => ThreadCommand::SetPreview {
            preview,
            first_user_message,
        },
        ThreadEventBody::AgentEventMetadataReconciled { generation } => {
            ThreadCommand::MarkAgentEventMetadataReconciled { generation }
        }
        ThreadEventBody::AgentEventTimelineReconciled {
            generation,
            created_at_unix_ms,
            updated_at_unix_ms,
        } => ThreadCommand::ReconcileAgentEventTimeline {
            generation,
            created_at_unix_ms,
            updated_at_unix_ms,
        },
    })
}

#[cfg(test)]
mod checkpoint_coverage_tests {
    use super::*;

    #[test]
    fn recovered_checkpoint_must_not_disappear_or_regress() {
        let checkpoint = ThreadRevision::new(8);
        assert_eq!(
            model_context_fold_coverage(None, None),
            ModelContextFoldCoverage::Complete
        );
        assert_eq!(
            model_context_fold_coverage(Some(checkpoint), Some(checkpoint)),
            ModelContextFoldCoverage::Checkpointed
        );
        assert_eq!(
            model_context_fold_coverage(Some(checkpoint), Some(ThreadRevision::new(12))),
            ModelContextFoldCoverage::Checkpointed
        );
        for recovered in [None, Some(ThreadRevision::new(7))] {
            assert_eq!(
                model_context_fold_coverage(Some(checkpoint), recovered),
                ModelContextFoldCoverage::CheckpointInvalidated
            );
        }
    }

    #[test]
    fn filesystem_presence_is_async_and_store_owned() {
        let source = include_str!("store.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("store production source");
        assert!(source.contains("pub async fn thread_exists"));
        assert!(source.contains("tokio::fs::metadata"));
        assert!(source.contains("tokio::fs::remove_dir_all"));
        assert!(!source.contains("pub fn contains_thread"));
        assert!(!source.contains("std::fs::remove_dir_all"));
    }

    #[test]
    fn recovery_initializer_borrows_the_index_without_cloning_it() {
        let source = include_str!("store.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("store production source");
        assert!(source.contains("pub struct ThreadOpenIndex<'a>"));
        assert!(source.contains("ThreadOpenIndex::new(indexed.as_ref())"));
        assert!(source.contains("initialize(open_index)"));
        assert!(source.contains("pub fn dynamic_tools_match(self, tools: &ContentRef)"));
        assert!(source.contains("projection.dynamic_tools_digest"));
        assert!(!source.contains("transcript_index.clone()"));
    }

    #[test]
    fn prepared_restore_consumes_its_projection_without_a_second_index_read() {
        let source = include_str!("store.rs")
            .split("pub async fn open_prepared_thread_and_fold")
            .nth(1)
            .expect("prepared restore")
            .split("async fn open_thread_and_fold_with_projection")
            .next()
            .expect("prepared restore body");

        assert!(source.contains("Some(prepared.projection)"));
        assert!(!source.contains("read_projection"));
        assert!(!source.contains("summary.clone()"));
    }

    #[test]
    fn prepared_source_fold_reuses_projection_and_never_rereads_the_index() {
        let source = include_str!("store.rs")
            .split("pub async fn fold_prepared_thread_events")
            .nth(1)
            .expect("prepared source fold")
            .split("pub async fn open_prepared_thread_and_fold")
            .next()
            .expect("prepared source fold body");

        assert!(source.contains("ThreadIndexAccumulator::from_projection"));
        assert!(!source.contains("read_projection"));
        assert!(!source.contains("read_summary"));
        assert!(source.contains("projection.summary.revision != prepared_revision"));
        assert!(source.contains("self.index.synchronize(&projection)"));
        assert_eq!(source.matches("fold_existing_thread_events(").count(), 1);
        assert!(source.contains("PreparedProjectionAhead"));
        assert!(source.contains("model_context_fold_coverage("));
    }

    #[test]
    fn prepared_projection_exposes_only_its_borrowed_restore_index() {
        let source = include_str!("store.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("store production source");

        assert!(source.contains("pub fn open_index(&self) -> ThreadOpenIndex<'_>"));
        assert!(source.contains("ThreadOpenIndex::new(Some(&self.projection))"));
        assert!(!source.contains("pub fn projection("));
    }

    #[test]
    fn missing_index_projection_has_one_rebuild_owner() {
        let source = include_str!("store.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("store production source");
        let prepare = source
            .split("pub async fn prepare_thread_projection")
            .nth(1)
            .expect("prepare projection")
            .split("pub async fn fold_prepared_thread_events")
            .next()
            .expect("prepare body");
        let summary = source
            .split("pub async fn read_summary")
            .nth(1)
            .expect("summary read")
            .split("pub async fn set_archived")
            .next()
            .expect("summary body");

        assert!(prepare.contains("self.rebuild_projection(thread_id).await?"));
        assert!(summary.contains("self.rebuild_projection(thread_id).await?"));
        assert_eq!(source.matches("async fn rebuild_projection(").count(), 1);
        assert_eq!(
            source
                .matches("self.index.replace(&projection).await?")
                .count(),
            1
        );
    }

    #[test]
    fn durable_fork_streams_replay_with_a_bounded_buffer() {
        let source = include_str!("store.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("store production source");
        let fork = source
            .split("pub async fn fork_thread")
            .nth(1)
            .expect("fork implementation")
            .split("fn rebuild_index")
            .next()
            .expect("fork body");

        assert!(fork.contains("mpsc::channel(FORK_REPLAY_BUFFER)"));
        assert!(fork.contains("tokio::join!(producer, consumer)"));
        assert!(fork.contains("ForkReplayRecord::Complete"));
        assert!(fork.contains("target_revision = receipt.revision_after"));
        assert!(fork.contains("rebase_fork_command(target_revision, command)"));
        assert!(!fork.contains("rebase_fork_command(&target, command).await"));
        assert!(source.contains("sender.blocking_send(record)"));
        assert!(!source.contains("commands: Vec<(ThreadRevision, ThreadCommand)>"));
        assert!(FORK_REPLAY_BUFFER <= 32);
    }
}

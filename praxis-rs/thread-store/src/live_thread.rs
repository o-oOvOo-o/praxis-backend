use crate::ThreadStoreError;
use crate::index::ThreadIndex;
use crate::projection::ThreadIndexAccumulator;
use praxis_thread_store_contracts::BatchId;
use praxis_thread_store_contracts::CommandId;
use praxis_thread_store_contracts::ContentRef;
use praxis_thread_store_contracts::EventId;
use praxis_thread_store_contracts::NewThreadEvent;
use praxis_thread_store_contracts::ThreadActor;
use praxis_thread_store_contracts::ThreadCommand;
use praxis_thread_store_contracts::ThreadCommandHeader;
use praxis_thread_store_contracts::ThreadCommandReceipt;
use praxis_thread_store_contracts::ThreadEventBody;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadHead;
use praxis_thread_store_contracts::ThreadId;
use praxis_thread_store_contracts::ThreadResumeConfig;
use praxis_thread_store_contracts::ThreadRevision;
use praxis_thread_store_contracts::ThreadRevisionRef;
use praxis_thread_store_journal::JournalBatch;
use praxis_thread_store_journal::JournalDurability;
use praxis_thread_store_journal::ThreadJournal;
use praxis_thread_store_journal::ThreadRevisionRange;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::sync::Mutex as AsyncMutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitMode {
    Buffered,
    Durable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadSessionMetadata {
    name: Option<String>,
    model: String,
    model_provider: Option<String>,
    reasoning_effort: Option<String>,
    dynamic_tools: ContentRef,
}

impl ThreadSessionMetadata {
    pub fn new(
        name: Option<String>,
        model: String,
        model_provider: Option<String>,
        reasoning_effort: Option<String>,
        dynamic_tools: ContentRef,
    ) -> Self {
        Self {
            name,
            model,
            model_provider,
            reasoning_effort,
            dynamic_tools,
        }
    }
}

pub(crate) struct ThreadSessionMetadataState {
    name: Option<String>,
    model: Option<String>,
    model_provider: Option<String>,
    reasoning_effort: Option<String>,
    dynamic_tools_digest: Option<praxis_thread_store_contracts::Digest>,
}

impl ThreadSessionMetadataState {
    pub(crate) fn new(
        name: Option<String>,
        model: Option<String>,
        model_provider: Option<String>,
        reasoning_effort: Option<String>,
        dynamic_tools_digest: Option<praxis_thread_store_contracts::Digest>,
    ) -> Self {
        Self {
            name,
            model,
            model_provider,
            reasoning_effort,
            dynamic_tools_digest,
        }
    }
}

#[derive(Clone)]
pub struct LiveThreadStore {
    thread_id: ThreadId,
    journal: Arc<Mutex<ThreadJournal>>,
    index: ThreadIndex,
    session_metadata: Arc<AsyncMutex<ThreadSessionMetadataState>>,
}

impl LiveThreadStore {
    pub(crate) fn new(
        thread_id: ThreadId,
        journal: Arc<Mutex<ThreadJournal>>,
        index: ThreadIndex,
        session_metadata: ThreadSessionMetadataState,
    ) -> Self {
        Self {
            thread_id,
            journal,
            index,
            session_metadata: Arc::new(AsyncMutex::new(session_metadata)),
        }
    }

    pub const fn thread_id(&self) -> ThreadId {
        self.thread_id
    }

    pub async fn ensure_created(
        &self,
        source: impl Into<String>,
        workspace: impl Into<String>,
        parent: Option<ThreadRevisionRef>,
    ) -> Result<ThreadCommandReceipt, ThreadStoreError> {
        self.execute(
            ThreadActor::Runtime,
            None,
            ThreadCommand::Create {
                source: source.into(),
                workspace: workspace.into(),
                parent,
            },
            CommitMode::Durable,
        )
        .await
    }

    pub async fn execute(
        &self,
        actor: ThreadActor,
        correlation_id: Option<String>,
        command: ThreadCommand,
        commit_mode: CommitMode,
    ) -> Result<ThreadCommandReceipt, ThreadStoreError> {
        let thread_id = self.thread_id;
        let journal = Arc::clone(&self.journal);
        let outcome = run_blocking(move || {
            let mut journal = journal
                .lock()
                .map_err(|_| ThreadStoreError::WriterPoisoned)?;
            execute_locked(
                &mut journal,
                thread_id,
                actor,
                correlation_id,
                command,
                commit_mode,
            )
        })
        .await?;
        for event in &outcome.events {
            if !self.index.apply(event).await? {
                self.synchronize_index().await?;
                break;
            }
        }
        Ok(outcome.receipt)
    }

    /// Atomically persist changed session metadata in one journal batch.
    pub async fn reconcile_session_metadata(
        &self,
        desired: ThreadSessionMetadata,
        commit_mode: CommitMode,
    ) -> Result<bool, ThreadStoreError> {
        let tools_digest = crate::projection::dynamic_tools_digest(&desired.dynamic_tools);
        let mut persisted = self.session_metadata.lock().await;
        let name = desired
            .name
            .as_ref()
            .filter(|name| persisted.name.as_ref() != Some(*name))
            .cloned();
        let resume_config = (persisted.model.as_deref() != Some(desired.model.as_str())
            || persisted.model_provider != desired.model_provider
            || persisted.reasoning_effort != desired.reasoning_effort)
            .then(|| ThreadResumeConfig {
                model: Some(desired.model.clone()),
                model_provider: desired.model_provider.clone(),
                reasoning_effort: desired.reasoning_effort.clone(),
            });
        let dynamic_tools = (persisted.dynamic_tools_digest != Some(tools_digest))
            .then(|| desired.dynamic_tools.clone());
        if name.is_none() && resume_config.is_none() && dynamic_tools.is_none() {
            return Ok(false);
        }
        self.execute(
            ThreadActor::Runtime,
            None,
            ThreadCommand::ReconcileSessionMetadata {
                name,
                resume_config,
                dynamic_tools,
            },
            commit_mode,
        )
        .await?;
        if desired.name.is_some() {
            persisted.name = desired.name;
        }
        persisted.model = Some(desired.model);
        persisted.model_provider = desired.model_provider;
        persisted.reasoning_effort = desired.reasoning_effort;
        persisted.dynamic_tools_digest = Some(tools_digest);
        Ok(true)
    }

    pub async fn head(&self) -> Result<ThreadHead, ThreadStoreError> {
        let journal = Arc::clone(&self.journal);
        run_blocking(move || {
            journal
                .lock()
                .map(|journal| journal.head())
                .map_err(|_| ThreadStoreError::WriterPoisoned)
        })
        .await
    }

    pub async fn next_agent_event_sequence(&self) -> Result<u64, ThreadStoreError> {
        let last = self
            .index
            .latest_agent_event_sequence(self.thread_id)
            .await?;
        Ok(last.saturating_add(1).max(1))
    }

    pub async fn agent_event_metadata_generation(&self) -> Result<u32, ThreadStoreError> {
        self.index
            .agent_event_metadata_generation(self.thread_id)
            .await
    }

    pub async fn summary(&self) -> Result<Option<crate::ThreadSummary>, ThreadStoreError> {
        self.index.read_summary(self.thread_id).await
    }

    pub async fn model_context_checkpoint(
        &self,
    ) -> Result<Option<ThreadRevision>, ThreadStoreError> {
        self.index.model_context_checkpoint(self.thread_id).await
    }

    /// Select one exact transcript checkpoint from a consistent index snapshot.
    pub async fn transcript_scan_plan<F>(
        &self,
        select_first_turn: F,
    ) -> Result<Option<crate::TranscriptScanPlan>, ThreadStoreError>
    where
        F: FnOnce(usize) -> Option<usize>,
    {
        self.index
            .transcript_scan_plan(self.thread_id, select_first_turn)
            .await
    }

    /// Fold journal events on the blocking reader without retaining an event vector.
    pub async fn fold_all<S, F>(&self, state: S, fold: F) -> Result<S, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &ThreadEventEnvelope) + Send + 'static,
    {
        self.fold_window(ThreadRevision::ZERO, None, state, fold)
            .await
    }

    /// Fold a durable journal prefix without retaining its event envelopes.
    pub async fn fold_through<S, F>(
        &self,
        through: ThreadRevision,
        state: S,
        fold: F,
    ) -> Result<S, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &ThreadEventEnvelope) + Send + 'static,
    {
        self.fold_window(ThreadRevision::ZERO, Some(through), state, fold)
            .await
    }

    /// Fold events committed after a revision checkpoint up to one head snapshot.
    pub async fn fold_after<S, F>(
        &self,
        after: ThreadRevision,
        state: S,
        fold: F,
    ) -> Result<S, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &ThreadEventEnvelope) + Send + 'static,
    {
        self.fold_window(after, None, state, fold).await
    }

    /// Fold one exact recovered revision window.
    pub async fn fold_between<S, F>(
        &self,
        after: ThreadRevision,
        through: ThreadRevision,
        state: S,
        fold: F,
    ) -> Result<S, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &ThreadEventEnvelope) + Send + 'static,
    {
        self.fold_window(after, Some(through), state, fold).await
    }

    async fn fold_window<S, F>(
        &self,
        after: ThreadRevision,
        through: Option<ThreadRevision>,
        state: S,
        fold: F,
    ) -> Result<S, ThreadStoreError>
    where
        S: Send + 'static,
        F: FnMut(&mut S, &ThreadEventEnvelope) + Send + 'static,
    {
        let journal = Arc::clone(&self.journal);
        run_blocking(move || {
            let snapshot = journal
                .lock()
                .map_err(|_| ThreadStoreError::WriterPoisoned)?
                .snapshot();
            let head = snapshot.head();
            let through = through.unwrap_or(head.revision);
            if through > head.revision {
                return Err(ThreadStoreError::RevisionNotFound(through.get()));
            }
            if after > through {
                return Err(ThreadStoreError::RevisionNotFound(after.get()));
            }
            if after == through {
                return Ok(state);
            }
            let start = after
                .checked_next()
                .ok_or(ThreadStoreError::RevisionOverflow)?;
            Ok(
                snapshot.fold_range(
                    ThreadRevisionRange::inclusive(start, through)?,
                    state,
                    fold,
                )?,
            )
        })
        .await
    }

    pub(crate) async fn synchronize_index(&self) -> Result<(), ThreadStoreError> {
        let accumulator = self
            .fold_all(
                ThreadIndexAccumulator::default(),
                ThreadIndexAccumulator::push,
            )
            .await?;
        if let Some(projection) = accumulator.finish() {
            self.index.replace(&projection).await?;
        }
        Ok(())
    }

    pub async fn sync(&self) -> Result<ThreadHead, ThreadStoreError> {
        let journal = Arc::clone(&self.journal);
        run_blocking(move || {
            let mut journal = journal
                .lock()
                .map_err(|_| ThreadStoreError::WriterPoisoned)?;
            Ok(journal.sync()?.through)
        })
        .await
    }
}

fn execute_locked(
    journal: &mut ThreadJournal,
    thread_id: ThreadId,
    actor: ThreadActor,
    correlation_id: Option<String>,
    command: ThreadCommand,
    commit_mode: CommitMode,
) -> Result<CommitOutcome, ThreadStoreError> {
    let head = journal.head();
    let recorded_at_unix_ms = now_unix_ms()?;
    let command_header = ThreadCommandHeader::new(
        CommandId::new(),
        thread_id,
        head.revision,
        &actor,
        &correlation_id,
        &command,
    );
    if matches!(command, ThreadCommand::Create { .. }) && head != ThreadHead::EMPTY {
        return Ok(CommitOutcome {
            receipt: ThreadCommandReceipt::no_op(
                command_header.command_id(),
                command_header.command_digest(),
                head.revision,
                recorded_at_unix_ms,
            ),
            events: Vec::new(),
        });
    }
    if !matches!(command, ThreadCommand::Create { .. }) && head == ThreadHead::EMPTY {
        return Err(ThreadStoreError::ThreadNotCreated);
    }
    let bodies = event_bodies(command);
    if bodies.is_empty() {
        return Ok(CommitOutcome {
            receipt: ThreadCommandReceipt::no_op(
                command_header.command_id(),
                command_header.command_digest(),
                head.revision,
                recorded_at_unix_ms,
            ),
            events: Vec::new(),
        });
    }
    let batch_id = BatchId::new();
    let mut revision = head.revision;
    let mut previous_record_digest = head.record_digest;
    let mut events = Vec::with_capacity(bodies.len());
    for (sequence, body) in bodies.into_iter().enumerate() {
        revision = revision
            .checked_next()
            .ok_or(ThreadStoreError::RevisionOverflow)?;
        let event = ThreadEventEnvelope::new(NewThreadEvent {
            thread_id,
            revision,
            event_id: EventId::new(),
            batch_id,
            sequence: u32::try_from(sequence).map_err(|_| ThreadStoreError::RevisionOverflow)?,
            recorded_at_unix_ms,
            actor: actor.clone(),
            correlation_id: correlation_id.clone(),
            causation_id: None,
            body,
            previous_record_digest,
        });
        previous_record_digest = event.record_digest;
        events.push(event);
    }
    let batch = JournalBatch::new(command_header, batch_id, recorded_at_unix_ms, events);
    let durability = match commit_mode {
        CommitMode::Buffered => JournalDurability::Buffered,
        CommitMode::Durable => JournalDurability::Durable,
    };
    let (receipt, committed_events) = journal.append(batch, durability)?.into_receipt_and_events();
    Ok(CommitOutcome {
        receipt,
        events: committed_events.unwrap_or_default(),
    })
}

struct CommitOutcome {
    receipt: ThreadCommandReceipt,
    events: Vec<ThreadEventEnvelope>,
}

fn event_bodies(command: ThreadCommand) -> Vec<ThreadEventBody> {
    match command {
        ThreadCommand::ReconcileSessionMetadata {
            name,
            resume_config,
            dynamic_tools,
        } => metadata_event_bodies(name, resume_config, dynamic_tools),
        command => vec![event_body(command)],
    }
}

fn metadata_event_bodies(
    name: Option<String>,
    resume_config: Option<ThreadResumeConfig>,
    dynamic_tools: Option<ContentRef>,
) -> Vec<ThreadEventBody> {
    let mut events = Vec::with_capacity(3);
    if let Some(name) = name {
        events.push(ThreadEventBody::ThreadNameSet { name: Some(name) });
    }
    if let Some(ThreadResumeConfig {
        model,
        model_provider,
        reasoning_effort,
    }) = resume_config
    {
        events.push(ThreadEventBody::ThreadResumeConfigSet {
            model,
            model_provider,
            reasoning_effort,
        });
    }
    if let Some(tools) = dynamic_tools {
        events.push(ThreadEventBody::ThreadDynamicToolsSet { tools });
    }
    events
}

fn event_body(command: ThreadCommand) -> ThreadEventBody {
    match command {
        ThreadCommand::Create {
            source,
            workspace,
            parent,
        } => ThreadEventBody::ThreadCreated {
            source,
            workspace,
            parent,
        },
        ThreadCommand::SetName { name } => ThreadEventBody::ThreadNameSet { name },
        ThreadCommand::SetSummary { summary } => ThreadEventBody::ThreadSummarySet { summary },
        ThreadCommand::SetArchived { archived } => ThreadEventBody::ThreadArchived { archived },
        ThreadCommand::SetWorkspace { workspace } => {
            ThreadEventBody::ThreadWorkspaceSet { workspace }
        }
        ThreadCommand::StartTurn {
            turn_id,
            collaboration_mode,
        } => ThreadEventBody::TurnStarted {
            turn_id,
            collaboration_mode,
        },
        ThreadCommand::CaptureTurnExecutionContext { turn_id, context } => {
            ThreadEventBody::TurnExecutionContextCaptured { turn_id, context }
        }
        ThreadCommand::RecordNativeAgentEvent {
            agent_sequence,
            event_id,
            turn_id,
            route,
            payload,
        } => ThreadEventBody::NativeAgentEventRecorded {
            agent_sequence,
            event_id,
            turn_id,
            route,
            payload,
        },
        ThreadCommand::AppendTranscriptItem {
            item_id,
            turn_id,
            item_kind,
            content,
        } => ThreadEventBody::TranscriptItemCreated {
            item_id,
            turn_id,
            item_kind,
            content,
        },
        ThreadCommand::FinalizeTranscriptItem { item_id, content } => {
            ThreadEventBody::TranscriptItemFinalized { item_id, content }
        }
        ThreadCommand::CancelTranscriptItem { item_id, reason } => {
            ThreadEventBody::TranscriptItemCancelled { item_id, reason }
        }
        ThreadCommand::CompleteTurn { turn_id } => ThreadEventBody::TurnCompleted { turn_id },
        ThreadCommand::AbortTurn { turn_id, reason } => {
            ThreadEventBody::TurnAborted { turn_id, reason }
        }
        ThreadCommand::FailTurn {
            turn_id,
            error_code,
            message,
        } => ThreadEventBody::TurnFailed {
            turn_id,
            error_code,
            message,
        },
        ThreadCommand::ReplaceModelContextBaseline {
            basis_revision,
            summary,
            retained_item_ids,
        } => ThreadEventBody::ModelContextBaselineReplaced {
            basis_revision,
            summary,
            retained_item_ids,
        },
        ThreadCommand::ReplaceModelContextSnapshot {
            basis_revision,
            snapshot,
        } => ThreadEventBody::ModelContextSnapshotReplaced {
            basis_revision,
            snapshot,
        },
        ThreadCommand::RollbackModelContext { user_turns } => {
            ThreadEventBody::ModelContextRolledBack { user_turns }
        }
        ThreadCommand::MoveTranscriptHead {
            from_revision,
            to_revision,
            reason,
        } => ThreadEventBody::TranscriptHeadMoved {
            from_revision,
            to_revision,
            reason,
        },
        ThreadCommand::RedactContent {
            item_id,
            replacement,
            reason,
        } => ThreadEventBody::ContentRedacted {
            item_id,
            replacement,
            reason,
        },
        ThreadCommand::RecordModelContextItem {
            item_id,
            turn_id,
            content,
        } => ThreadEventBody::ModelContextItemRecorded {
            item_id,
            turn_id,
            content,
        },
        ThreadCommand::RecordTurnCost { cost_micros } => {
            ThreadEventBody::TurnCostRecorded { cost_micros }
        }
        ThreadCommand::SetResumeConfig {
            model,
            model_provider,
            reasoning_effort,
        } => ThreadEventBody::ThreadResumeConfigSet {
            model,
            model_provider,
            reasoning_effort,
        },
        ThreadCommand::SetDynamicTools { tools } => {
            ThreadEventBody::ThreadDynamicToolsSet { tools }
        }
        ThreadCommand::SetPreview {
            preview,
            first_user_message,
        } => ThreadEventBody::ThreadPreviewSet {
            preview,
            first_user_message,
        },
        ThreadCommand::MarkAgentEventMetadataReconciled { generation } => {
            ThreadEventBody::AgentEventMetadataReconciled { generation }
        }
        ThreadCommand::ReconcileAgentEventTimeline {
            generation,
            created_at_unix_ms,
            updated_at_unix_ms,
        } => ThreadEventBody::AgentEventTimelineReconciled {
            generation,
            created_at_unix_ms,
            updated_at_unix_ms,
        },
        ThreadCommand::ReconcileSessionMetadata { .. } => {
            unreachable!("metadata reconciliation is projected as an event batch")
        }
    }
}

fn now_unix_ms() -> Result<i64, ThreadStoreError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ThreadStoreError::InvalidClock)?
        .as_millis();
    i64::try_from(millis).map_err(|_| ThreadStoreError::InvalidClock)
}

async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, ThreadStoreError> + Send + 'static,
) -> Result<T, ThreadStoreError> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| ThreadStoreError::Worker(error.to_string()))?
}

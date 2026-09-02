use crate::BatchId;
use crate::CanonicalEncode;
use crate::CanonicalHasher;
use crate::Digest;
use crate::EventId;
use crate::ItemId;
use crate::THREAD_STORE_SCHEMA_VERSION;
use crate::ThreadId;
use crate::ThreadRevision;
use crate::ThreadRevisionRef;
use crate::TurnId;
use serde::Deserialize;
use serde::Serialize;

const EVENT_PAYLOAD_DOMAIN: &str = "praxis.thread-store.event-payload.v1";
const EVENT_RECORD_DOMAIN: &str = "praxis.thread-store.event-record.v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ThreadActor {
    User,
    Agent,
    Runtime,
    System,
    Importer(String),
}

impl CanonicalEncode for ThreadActor {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        match self {
            Self::User => hasher.u8(0),
            Self::Agent => hasher.u8(1),
            Self::Runtime => hasher.u8(2),
            Self::System => hasher.u8(3),
            Self::Importer(importer) => {
                hasher.u8(4);
                importer.encode_canonical(hasher);
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentRef {
    InlineText {
        text: String,
    },
    Artifact {
        digest: Digest,
        bytes: u64,
        media_type: String,
    },
}

impl CanonicalEncode for ContentRef {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        match self {
            Self::InlineText { text } => {
                hasher.u8(0);
                text.encode_canonical(hasher);
            }
            Self::Artifact {
                digest,
                bytes,
                media_type,
            } => {
                hasher.u8(1);
                digest.encode_canonical(hasher);
                bytes.encode_canonical(hasher);
                media_type.encode_canonical(hasher);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptItemKind {
    UserMessage,
    AssistantMessage,
    ReasoningSummary,
    ToolCall,
    ToolResult,
    FileChange,
    CollaborationAction,
    SystemNotice,
    OpaqueImported,
}

impl CanonicalEncode for TranscriptItemKind {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.u8(match self {
            Self::UserMessage => 0,
            Self::AssistantMessage => 1,
            Self::ReasoningSummary => 2,
            Self::ToolCall => 3,
            Self::ToolResult => 4,
            Self::FileChange => 5,
            Self::CollaborationAction => 6,
            Self::SystemNotice => 7,
            Self::OpaqueImported => 8,
        });
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum TurnAbortReason {
    Interrupted,
    Replaced,
    ReviewEnded,
    Other(String),
}

/// Lightweight native routing for opaque agent-event payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventRoute {
    TurnStarted,
    UserMessage,
    AssistantMessage,
    Transcript,
    Other,
}

impl AgentEventRoute {
    pub const fn affects_transcript(self) -> bool {
        !matches!(self, Self::Other)
    }

    pub const fn affects_conversation(self) -> bool {
        matches!(self, Self::UserMessage | Self::AssistantMessage)
    }
}

impl CanonicalEncode for AgentEventRoute {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.u8(match self {
            Self::TurnStarted => 0,
            Self::UserMessage => 1,
            Self::AssistantMessage => 2,
            Self::Transcript => 3,
            Self::Other => 4,
        });
    }
}

impl CanonicalEncode for TurnAbortReason {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        match self {
            Self::Interrupted => hasher.u8(0),
            Self::Replaced => hasher.u8(1),
            Self::ReviewEnded => hasher.u8(2),
            Self::Other(detail) => {
                hasher.u8(3);
                detail.encode_canonical(hasher);
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ThreadEventBody {
    ThreadCreated {
        source: String,
        workspace: String,
        parent: Option<ThreadRevisionRef>,
    },
    ThreadNameSet {
        name: Option<String>,
    },
    ThreadSummarySet {
        summary: Option<String>,
    },
    ThreadArchived {
        archived: bool,
    },
    ThreadWorkspaceSet {
        workspace: String,
    },
    TurnStarted {
        turn_id: TurnId,
        collaboration_mode: Option<String>,
    },
    TurnExecutionContextCaptured {
        turn_id: TurnId,
        context: ContentRef,
    },
    NativeAgentEventRecorded {
        agent_sequence: u64,
        event_id: String,
        turn_id: Option<TurnId>,
        route: AgentEventRoute,
        payload: ContentRef,
    },
    TranscriptItemCreated {
        item_id: ItemId,
        turn_id: Option<TurnId>,
        item_kind: TranscriptItemKind,
        content: ContentRef,
    },
    TranscriptItemFinalized {
        item_id: ItemId,
        content: ContentRef,
    },
    TranscriptItemCancelled {
        item_id: ItemId,
        reason: String,
    },
    TurnCompleted {
        turn_id: TurnId,
    },
    TurnAborted {
        turn_id: TurnId,
        reason: TurnAbortReason,
    },
    TurnFailed {
        turn_id: TurnId,
        error_code: String,
        message: String,
    },
    ModelContextBaselineReplaced {
        basis_revision: ThreadRevision,
        summary: ContentRef,
        retained_item_ids: Vec<ItemId>,
    },
    ModelContextSnapshotReplaced {
        basis_revision: ThreadRevision,
        snapshot: ContentRef,
    },
    ModelContextRolledBack {
        user_turns: u32,
    },
    TranscriptHeadMoved {
        from_revision: ThreadRevision,
        to_revision: ThreadRevision,
        reason: String,
    },
    ContentRedacted {
        item_id: ItemId,
        replacement: Option<ContentRef>,
        reason: String,
    },
    ExternalHistoryImported {
        source_format: String,
        source_fingerprint: Digest,
        importer_id: String,
        importer_version: String,
        imported_event_count: u64,
        warning_count: u64,
        source: Option<ContentRef>,
    },
    OpaqueImportedEvent {
        source_format: String,
        original_type: String,
        payload: ContentRef,
    },
    ModelContextItemRecorded {
        item_id: ItemId,
        turn_id: Option<TurnId>,
        content: ContentRef,
    },
    TurnCostRecorded {
        cost_micros: Option<i64>,
    },
    ThreadResumeConfigSet {
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_provider: Option<String>,
        reasoning_effort: Option<String>,
    },
    ThreadDynamicToolsSet {
        tools: ContentRef,
    },
}

impl ThreadEventBody {
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::ThreadCreated { .. } => "thread_created",
            Self::ThreadNameSet { .. } => "thread_name_set",
            Self::ThreadSummarySet { .. } => "thread_summary_set",
            Self::ThreadArchived { .. } => "thread_archived",
            Self::ThreadWorkspaceSet { .. } => "thread_workspace_set",
            Self::TurnStarted { .. } => "turn_started",
            Self::TurnExecutionContextCaptured { .. } => "turn_execution_context_captured",
            Self::NativeAgentEventRecorded { .. } => "native_agent_event_recorded",
            Self::TranscriptItemCreated { .. } => "transcript_item_created",
            Self::TranscriptItemFinalized { .. } => "transcript_item_finalized",
            Self::TranscriptItemCancelled { .. } => "transcript_item_cancelled",
            Self::TurnCompleted { .. } => "turn_completed",
            Self::TurnAborted { .. } => "turn_aborted",
            Self::TurnFailed { .. } => "turn_failed",
            Self::ModelContextBaselineReplaced { .. } => "model_context_baseline_replaced",
            Self::ModelContextSnapshotReplaced { .. } => "model_context_snapshot_replaced",
            Self::ModelContextRolledBack { .. } => "model_context_rolled_back",
            Self::TranscriptHeadMoved { .. } => "transcript_head_moved",
            Self::ContentRedacted { .. } => "content_redacted",
            Self::ExternalHistoryImported { .. } => "external_history_imported",
            Self::OpaqueImportedEvent { .. } => "opaque_imported_event",
            Self::ModelContextItemRecorded { .. } => "model_context_item_recorded",
            Self::TurnCostRecorded { .. } => "turn_cost_recorded",
            Self::ThreadResumeConfigSet { .. } => "thread_resume_config_set",
            Self::ThreadDynamicToolsSet { .. } => "thread_dynamic_tools_set",
        }
    }
}

impl CanonicalEncode for ThreadEventBody {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        self.type_name().encode_canonical(hasher);
        match self {
            Self::ThreadCreated {
                source,
                workspace,
                parent,
            } => {
                source.encode_canonical(hasher);
                workspace.encode_canonical(hasher);
                parent.encode_canonical(hasher);
            }
            Self::ThreadNameSet { name } => name.encode_canonical(hasher),
            Self::ThreadSummarySet { summary } => summary.encode_canonical(hasher),
            Self::ThreadArchived { archived } => archived.encode_canonical(hasher),
            Self::ThreadWorkspaceSet { workspace } => workspace.encode_canonical(hasher),
            Self::TurnStarted {
                turn_id,
                collaboration_mode,
            } => {
                turn_id.encode_canonical(hasher);
                collaboration_mode.encode_canonical(hasher);
            }
            Self::TurnExecutionContextCaptured { turn_id, context } => {
                turn_id.encode_canonical(hasher);
                context.encode_canonical(hasher);
            }
            Self::NativeAgentEventRecorded {
                agent_sequence,
                event_id,
                turn_id,
                route,
                payload,
            } => {
                agent_sequence.encode_canonical(hasher);
                event_id.encode_canonical(hasher);
                turn_id.encode_canonical(hasher);
                route.encode_canonical(hasher);
                payload.encode_canonical(hasher);
            }
            Self::TranscriptItemCreated {
                item_id,
                turn_id,
                item_kind,
                content,
            } => {
                item_id.encode_canonical(hasher);
                turn_id.encode_canonical(hasher);
                item_kind.encode_canonical(hasher);
                content.encode_canonical(hasher);
            }
            Self::TranscriptItemFinalized { item_id, content } => {
                item_id.encode_canonical(hasher);
                content.encode_canonical(hasher);
            }
            Self::TranscriptItemCancelled { item_id, reason } => {
                item_id.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::TurnCompleted { turn_id } => turn_id.encode_canonical(hasher),
            Self::TurnAborted { turn_id, reason } => {
                turn_id.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::TurnFailed {
                turn_id,
                error_code,
                message,
            } => {
                turn_id.encode_canonical(hasher);
                error_code.encode_canonical(hasher);
                message.encode_canonical(hasher);
            }
            Self::ModelContextBaselineReplaced {
                basis_revision,
                summary,
                retained_item_ids,
            } => {
                basis_revision.encode_canonical(hasher);
                summary.encode_canonical(hasher);
                retained_item_ids.encode_canonical(hasher);
            }
            Self::ModelContextSnapshotReplaced {
                basis_revision,
                snapshot,
            } => {
                basis_revision.encode_canonical(hasher);
                snapshot.encode_canonical(hasher);
            }
            Self::ModelContextRolledBack { user_turns } => {
                user_turns.encode_canonical(hasher);
            }
            Self::TranscriptHeadMoved {
                from_revision,
                to_revision,
                reason,
            } => {
                from_revision.encode_canonical(hasher);
                to_revision.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::ContentRedacted {
                item_id,
                replacement,
                reason,
            } => {
                item_id.encode_canonical(hasher);
                replacement.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::ExternalHistoryImported {
                source_format,
                source_fingerprint,
                importer_id,
                importer_version,
                imported_event_count,
                warning_count,
                source,
            } => {
                source_format.encode_canonical(hasher);
                source_fingerprint.encode_canonical(hasher);
                importer_id.encode_canonical(hasher);
                importer_version.encode_canonical(hasher);
                imported_event_count.encode_canonical(hasher);
                warning_count.encode_canonical(hasher);
                source.encode_canonical(hasher);
            }
            Self::OpaqueImportedEvent {
                source_format,
                original_type,
                payload,
            } => {
                source_format.encode_canonical(hasher);
                original_type.encode_canonical(hasher);
                payload.encode_canonical(hasher);
            }
            Self::ModelContextItemRecorded {
                item_id,
                turn_id,
                content,
            } => {
                item_id.encode_canonical(hasher);
                turn_id.encode_canonical(hasher);
                content.encode_canonical(hasher);
            }
            Self::TurnCostRecorded { cost_micros } => cost_micros.encode_canonical(hasher),
            Self::ThreadResumeConfigSet {
                model,
                model_provider,
                reasoning_effort,
            } => {
                model.encode_canonical(hasher);
                reasoning_effort.encode_canonical(hasher);
                if let Some(model_provider) = model_provider.as_ref() {
                    model_provider.encode_canonical(hasher);
                }
            }
            Self::ThreadDynamicToolsSet { tools } => tools.encode_canonical(hasher),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadEventEnvelope {
    pub schema_version: u32,
    pub thread_id: ThreadId,
    pub revision: ThreadRevision,
    pub event_id: EventId,
    pub batch_id: BatchId,
    pub sequence: u32,
    pub recorded_at_unix_ms: i64,
    pub actor: ThreadActor,
    pub correlation_id: Option<String>,
    pub causation_id: Option<EventId>,
    pub body: ThreadEventBody,
    pub payload_digest: Digest,
    pub previous_record_digest: Digest,
    pub record_digest: Digest,
}

pub struct NewThreadEvent {
    pub thread_id: ThreadId,
    pub revision: ThreadRevision,
    pub event_id: EventId,
    pub batch_id: BatchId,
    pub sequence: u32,
    pub recorded_at_unix_ms: i64,
    pub actor: ThreadActor,
    pub correlation_id: Option<String>,
    pub causation_id: Option<EventId>,
    pub body: ThreadEventBody,
    pub previous_record_digest: Digest,
}

impl ThreadEventEnvelope {
    pub fn new(event: NewThreadEvent) -> Self {
        let payload_digest = event.body.canonical_digest(EVENT_PAYLOAD_DOMAIN);
        let mut envelope = Self {
            schema_version: THREAD_STORE_SCHEMA_VERSION,
            thread_id: event.thread_id,
            revision: event.revision,
            event_id: event.event_id,
            batch_id: event.batch_id,
            sequence: event.sequence,
            recorded_at_unix_ms: event.recorded_at_unix_ms,
            actor: event.actor,
            correlation_id: event.correlation_id,
            causation_id: event.causation_id,
            body: event.body,
            payload_digest,
            previous_record_digest: event.previous_record_digest,
            record_digest: Digest::ZERO,
        };
        envelope.record_digest = envelope.compute_record_digest();
        envelope
    }

    pub fn compute_record_digest(&self) -> Digest {
        let mut hasher = CanonicalHasher::domain(EVENT_RECORD_DOMAIN);
        self.schema_version.encode_canonical(&mut hasher);
        self.thread_id.encode_canonical(&mut hasher);
        self.revision.encode_canonical(&mut hasher);
        self.event_id.encode_canonical(&mut hasher);
        self.batch_id.encode_canonical(&mut hasher);
        self.sequence.encode_canonical(&mut hasher);
        self.recorded_at_unix_ms.encode_canonical(&mut hasher);
        self.actor.encode_canonical(&mut hasher);
        self.correlation_id.encode_canonical(&mut hasher);
        self.causation_id.encode_canonical(&mut hasher);
        self.payload_digest.encode_canonical(&mut hasher);
        self.previous_record_digest.encode_canonical(&mut hasher);
        hasher.finish()
    }

    pub fn has_valid_digests(&self) -> bool {
        self.payload_digest == self.body.canonical_digest(EVENT_PAYLOAD_DOMAIN)
            && self.record_digest == self.compute_record_digest()
    }
}

use crate::CanonicalEncode;
use crate::CanonicalHasher;
use crate::CommandId;
use crate::ContentRef;
use crate::Digest;
use crate::ItemId;
use crate::THREAD_STORE_SCHEMA_VERSION;
use crate::ThreadActor;
use crate::ThreadId;
use crate::ThreadRevision;
use crate::ThreadRevisionRef;
use crate::TranscriptItemKind;
use crate::TurnAbortReason;
use crate::TurnId;
use serde::Deserialize;
use serde::Serialize;

const COMMAND_DOMAIN: &str = "praxis.thread-store.command.v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadResumeConfig {
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    pub reasoning_effort: Option<String>,
}

impl CanonicalEncode for ThreadResumeConfig {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        self.model.encode_canonical(hasher);
        self.reasoning_effort.encode_canonical(hasher);
        if let Some(model_provider) = self.model_provider.as_ref() {
            model_provider.encode_canonical(hasher);
        }
    }
}

/// Compact command identity used after canonical content has been hashed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadCommandHeader {
    command_id: CommandId,
    thread_id: ThreadId,
    expected_revision: ThreadRevision,
    command_digest: Digest,
}

impl ThreadCommandHeader {
    pub fn new(
        command_id: CommandId,
        thread_id: ThreadId,
        expected_revision: ThreadRevision,
        actor: &ThreadActor,
        correlation_id: &Option<String>,
        command: &ThreadCommand,
    ) -> Self {
        Self {
            command_id,
            thread_id,
            expected_revision,
            command_digest: compute_command_digest(
                THREAD_STORE_SCHEMA_VERSION,
                command_id,
                thread_id,
                expected_revision,
                actor,
                correlation_id,
                command,
            ),
        }
    }

    pub const fn command_id(self) -> CommandId {
        self.command_id
    }

    pub const fn thread_id(self) -> ThreadId {
        self.thread_id
    }

    pub const fn expected_revision(self) -> ThreadRevision {
        self.expected_revision
    }

    pub const fn command_digest(self) -> Digest {
        self.command_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ThreadCommand {
    Create {
        source: String,
        workspace: String,
        parent: Option<ThreadRevisionRef>,
    },
    SetName {
        name: Option<String>,
    },
    SetSummary {
        summary: Option<String>,
    },
    SetArchived {
        archived: bool,
    },
    SetWorkspace {
        workspace: String,
    },
    StartTurn {
        turn_id: TurnId,
        collaboration_mode: Option<String>,
    },
    CaptureTurnExecutionContext {
        turn_id: TurnId,
        context: ContentRef,
    },
    RecordNativeAgentEvent {
        agent_sequence: u64,
        event_id: String,
        turn_id: Option<TurnId>,
        route: crate::AgentEventRoute,
        payload: ContentRef,
    },
    AppendTranscriptItem {
        item_id: ItemId,
        turn_id: Option<TurnId>,
        item_kind: TranscriptItemKind,
        content: ContentRef,
    },
    FinalizeTranscriptItem {
        item_id: ItemId,
        content: ContentRef,
    },
    CancelTranscriptItem {
        item_id: ItemId,
        reason: String,
    },
    CompleteTurn {
        turn_id: TurnId,
    },
    AbortTurn {
        turn_id: TurnId,
        reason: TurnAbortReason,
    },
    FailTurn {
        turn_id: TurnId,
        error_code: String,
        message: String,
    },
    ReplaceModelContextBaseline {
        basis_revision: ThreadRevision,
        summary: ContentRef,
        retained_item_ids: Vec<ItemId>,
    },
    ReplaceModelContextSnapshot {
        basis_revision: ThreadRevision,
        snapshot: ContentRef,
    },
    RollbackModelContext {
        user_turns: u32,
    },
    MoveTranscriptHead {
        from_revision: ThreadRevision,
        to_revision: ThreadRevision,
        reason: String,
    },
    RedactContent {
        item_id: ItemId,
        replacement: Option<ContentRef>,
        reason: String,
    },
    RecordModelContextItem {
        item_id: ItemId,
        turn_id: Option<TurnId>,
        content: ContentRef,
    },
    RecordTurnCost {
        cost_micros: Option<i64>,
    },
    SetResumeConfig {
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_provider: Option<String>,
        reasoning_effort: Option<String>,
    },
    SetDynamicTools {
        tools: ContentRef,
    },
    ReconcileSessionMetadata {
        name: Option<String>,
        resume_config: Option<ThreadResumeConfig>,
        dynamic_tools: Option<ContentRef>,
    },
}

impl CanonicalEncode for ThreadCommand {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        match self {
            Self::Create {
                source,
                workspace,
                parent,
            } => {
                hasher.u8(0);
                source.encode_canonical(hasher);
                workspace.encode_canonical(hasher);
                parent.encode_canonical(hasher);
            }
            Self::SetName { name } => {
                hasher.u8(1);
                name.encode_canonical(hasher);
            }
            Self::SetSummary { summary } => {
                hasher.u8(2);
                summary.encode_canonical(hasher);
            }
            Self::SetArchived { archived } => {
                hasher.u8(3);
                archived.encode_canonical(hasher);
            }
            Self::StartTurn {
                turn_id,
                collaboration_mode,
            } => {
                hasher.u8(4);
                turn_id.encode_canonical(hasher);
                collaboration_mode.encode_canonical(hasher);
            }
            Self::CaptureTurnExecutionContext { turn_id, context } => {
                hasher.u8(5);
                turn_id.encode_canonical(hasher);
                context.encode_canonical(hasher);
            }
            Self::AppendTranscriptItem {
                item_id,
                turn_id,
                item_kind,
                content,
            } => {
                hasher.u8(6);
                item_id.encode_canonical(hasher);
                turn_id.encode_canonical(hasher);
                item_kind.encode_canonical(hasher);
                content.encode_canonical(hasher);
            }
            Self::FinalizeTranscriptItem { item_id, content } => {
                hasher.u8(7);
                item_id.encode_canonical(hasher);
                content.encode_canonical(hasher);
            }
            Self::CancelTranscriptItem { item_id, reason } => {
                hasher.u8(8);
                item_id.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::CompleteTurn { turn_id } => {
                hasher.u8(9);
                turn_id.encode_canonical(hasher);
            }
            Self::AbortTurn { turn_id, reason } => {
                hasher.u8(10);
                turn_id.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::FailTurn {
                turn_id,
                error_code,
                message,
            } => {
                hasher.u8(11);
                turn_id.encode_canonical(hasher);
                error_code.encode_canonical(hasher);
                message.encode_canonical(hasher);
            }
            Self::ReplaceModelContextBaseline {
                basis_revision,
                summary,
                retained_item_ids,
            } => {
                hasher.u8(12);
                basis_revision.encode_canonical(hasher);
                summary.encode_canonical(hasher);
                retained_item_ids.encode_canonical(hasher);
            }
            Self::MoveTranscriptHead {
                from_revision,
                to_revision,
                reason,
            } => {
                hasher.u8(13);
                from_revision.encode_canonical(hasher);
                to_revision.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::RedactContent {
                item_id,
                replacement,
                reason,
            } => {
                hasher.u8(14);
                item_id.encode_canonical(hasher);
                replacement.encode_canonical(hasher);
                reason.encode_canonical(hasher);
            }
            Self::RecordModelContextItem {
                item_id,
                turn_id,
                content,
            } => {
                hasher.u8(16);
                item_id.encode_canonical(hasher);
                turn_id.encode_canonical(hasher);
                content.encode_canonical(hasher);
            }
            Self::SetWorkspace { workspace } => {
                // Append-only command tag: existing persisted command digests must remain stable.
                hasher.u8(17);
                workspace.encode_canonical(hasher);
            }
            Self::RecordTurnCost { cost_micros } => {
                hasher.u8(18);
                cost_micros.encode_canonical(hasher);
            }
            Self::SetResumeConfig {
                model,
                model_provider,
                reasoning_effort,
            } => {
                hasher.u8(19);
                model.encode_canonical(hasher);
                reasoning_effort.encode_canonical(hasher);
                if let Some(model_provider) = model_provider.as_ref() {
                    model_provider.encode_canonical(hasher);
                }
            }
            Self::RollbackModelContext { user_turns } => {
                // Append-only command tag: existing persisted command digests must remain stable.
                hasher.u8(20);
                user_turns.encode_canonical(hasher);
            }
            Self::ReplaceModelContextSnapshot {
                basis_revision,
                snapshot,
            } => {
                hasher.u8(21);
                basis_revision.encode_canonical(hasher);
                snapshot.encode_canonical(hasher);
            }
            Self::RecordNativeAgentEvent {
                agent_sequence,
                event_id,
                turn_id,
                route,
                payload,
            } => {
                hasher.u8(22);
                agent_sequence.encode_canonical(hasher);
                event_id.encode_canonical(hasher);
                turn_id.encode_canonical(hasher);
                route.encode_canonical(hasher);
                payload.encode_canonical(hasher);
            }
            Self::SetDynamicTools { tools } => {
                hasher.u8(23);
                tools.encode_canonical(hasher);
            }
            Self::ReconcileSessionMetadata {
                name,
                resume_config,
                dynamic_tools,
            } => {
                hasher.u8(24);
                name.encode_canonical(hasher);
                resume_config.encode_canonical(hasher);
                dynamic_tools.encode_canonical(hasher);
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadCommandEnvelope {
    pub schema_version: u32,
    pub command_id: CommandId,
    pub thread_id: ThreadId,
    pub expected_revision: ThreadRevision,
    pub actor: ThreadActor,
    pub correlation_id: Option<String>,
    pub command: ThreadCommand,
    pub command_digest: Digest,
}

impl ThreadCommandEnvelope {
    pub fn new(
        command_id: CommandId,
        thread_id: ThreadId,
        expected_revision: ThreadRevision,
        actor: ThreadActor,
        correlation_id: Option<String>,
        command: ThreadCommand,
    ) -> Self {
        let command_digest = compute_command_digest(
            THREAD_STORE_SCHEMA_VERSION,
            command_id,
            thread_id,
            expected_revision,
            &actor,
            &correlation_id,
            &command,
        );
        Self {
            schema_version: THREAD_STORE_SCHEMA_VERSION,
            command_id,
            thread_id,
            expected_revision,
            actor,
            correlation_id,
            command,
            command_digest,
        }
    }

    pub fn compute_digest(&self) -> Digest {
        compute_command_digest(
            self.schema_version,
            self.command_id,
            self.thread_id,
            self.expected_revision,
            &self.actor,
            &self.correlation_id,
            &self.command,
        )
    }

    pub fn has_valid_digest(&self) -> bool {
        self.command_digest == self.compute_digest()
    }
}

fn compute_command_digest(
    schema_version: u32,
    command_id: CommandId,
    thread_id: ThreadId,
    expected_revision: ThreadRevision,
    actor: &ThreadActor,
    correlation_id: &Option<String>,
    command: &ThreadCommand,
) -> Digest {
    let mut hasher = CanonicalHasher::domain(COMMAND_DOMAIN);
    schema_version.encode_canonical(&mut hasher);
    command_id.encode_canonical(&mut hasher);
    thread_id.encode_canonical(&mut hasher);
    expected_revision.encode_canonical(&mut hasher);
    actor.encode_canonical(&mut hasher);
    correlation_id.encode_canonical(&mut hasher);
    command.encode_canonical(&mut hasher);
    hasher.finish()
}

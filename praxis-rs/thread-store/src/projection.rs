use praxis_thread_store_contracts::CanonicalEncode;
use praxis_thread_store_contracts::ContentRef;
use praxis_thread_store_contracts::Digest;
use praxis_thread_store_contracts::ThreadEventBody;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadId;
use praxis_thread_store_contracts::ThreadRevision;
use serde::Deserialize;
use serde::Serialize;
use std::collections::VecDeque;

const RECENT_TURN_CHECKPOINT_CAPACITY: usize = 256;
const DYNAMIC_TOOLS_DIGEST_DOMAIN: &str = "praxis.thread-store.dynamic-tools.v1";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThreadListSort {
    CreatedAt,
    #[default]
    UpdatedAt,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThreadListQuery {
    pub archived: Option<bool>,
    pub workspace: Option<String>,
    pub sources: Option<Vec<String>>,
    pub model_providers: Option<Vec<String>>,
    pub search: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub sort: ThreadListSort,
}

impl ThreadListQuery {
    pub fn set_cursor_after(&mut self, sort_value_unix_ms: i64, thread_id: ThreadId) {
        self.cursor = Some(format!(
            "v1:{}:{sort_value_unix_ms}:{thread_id}",
            match self.sort {
                ThreadListSort::CreatedAt => "c",
                ThreadListSort::UpdatedAt => "u",
            }
        ));
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub thread_id: ThreadId,
    pub revision: ThreadRevision,
    pub source: String,
    pub workspace: String,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub archived: bool,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_user_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost_micros: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_cost_micros: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadListPage {
    pub items: Vec<ThreadSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ThreadIndexProjection {
    pub(crate) summary: ThreadSummary,
    pub(crate) last_agent_sequence: u64,
    pub(crate) model_context_checkpoint: Option<ThreadRevision>,
    pub(crate) dynamic_tools_digest: Option<Digest>,
    pub(crate) agent_event_metadata_generation: u32,
    pub(crate) transcript_index: NativeTranscriptIndex,
}

/// Bounded, rebuildable transcript accelerator owned by ThreadStore.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeTranscriptIndex {
    pub(crate) total_turns: u64,
    pub(crate) checkpoints: VecDeque<TurnCheckpoint>,
    pub(crate) through_revision: ThreadRevision,
    pub(crate) frontier_sequence: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TurnCheckpoint {
    pub(crate) ordinal: u64,
    pub(crate) scan_after: ThreadRevision,
    pub(crate) previous_agent_sequence: Option<u64>,
}

/// Exact journal boundary for a bounded native transcript scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptScanPlan {
    pub turns_before: usize,
    pub scan_after: ThreadRevision,
    pub through_revision: ThreadRevision,
    pub previous_agent_sequence: Option<u64>,
}

impl NativeTranscriptIndex {
    pub fn total_turns(&self) -> usize {
        usize::try_from(self.total_turns).unwrap_or(usize::MAX)
    }

    pub fn plan_from(&self, first_turn: usize) -> Option<TranscriptScanPlan> {
        let checkpoint = u64::try_from(first_turn).ok().and_then(|first_turn| {
            self.checkpoints
                .iter()
                .find(|checkpoint| checkpoint.ordinal == first_turn.min(self.total_turns))
                .copied()
        });
        self.plan_from_checkpoint(first_turn, checkpoint)
    }

    pub(crate) fn plan_from_checkpoint(
        &self,
        first_turn: usize,
        checkpoint: Option<TurnCheckpoint>,
    ) -> Option<TranscriptScanPlan> {
        let first_turn = u64::try_from(first_turn).ok()?.min(self.total_turns);
        if first_turn == 0 {
            return Some(TranscriptScanPlan {
                turns_before: 0,
                scan_after: ThreadRevision::ZERO,
                through_revision: self.through_revision,
                previous_agent_sequence: None,
            });
        }
        if first_turn == self.total_turns {
            return Some(TranscriptScanPlan {
                turns_before: usize::try_from(first_turn).ok()?,
                scan_after: self.through_revision,
                through_revision: self.through_revision,
                previous_agent_sequence: self.frontier_sequence,
            });
        }
        let checkpoint = checkpoint.filter(|checkpoint| checkpoint.ordinal == first_turn)?;
        Some(TranscriptScanPlan {
            turns_before: usize::try_from(first_turn).ok()?,
            scan_after: checkpoint.scan_after,
            through_revision: self.through_revision,
            previous_agent_sequence: checkpoint.previous_agent_sequence,
        })
    }

    fn native_turn_started(
        &mut self,
        revision: ThreadRevision,
        previous_agent_sequence: Option<u64>,
    ) {
        let checkpoint = TurnCheckpoint {
            ordinal: self.total_turns,
            scan_after: ThreadRevision::new(revision.get().saturating_sub(1)),
            previous_agent_sequence,
        };
        self.total_turns = self.total_turns.saturating_add(1);
        self.checkpoints.push_back(checkpoint);
        while self.checkpoints.len() > RECENT_TURN_CHECKPOINT_CAPACITY {
            self.checkpoints.pop_front();
        }
    }
}

impl ThreadIndexProjection {
    pub(crate) fn from_created(event: &ThreadEventEnvelope) -> Option<Self> {
        let ThreadEventBody::ThreadCreated {
            source, workspace, ..
        } = &event.body
        else {
            return None;
        };
        Some(Self {
            summary: ThreadSummary {
                thread_id: event.thread_id,
                revision: event.revision,
                source: source.clone(),
                workspace: workspace.clone(),
                name: None,
                summary: None,
                archived: false,
                created_at_unix_ms: event.recorded_at_unix_ms,
                updated_at_unix_ms: event.recorded_at_unix_ms,
                preview: None,
                first_user_message: None,
                total_cost_micros: None,
                last_cost_micros: None,
                model: None,
                model_provider: None,
                reasoning_effort: None,
            },
            last_agent_sequence: 0,
            model_context_checkpoint: None,
            dynamic_tools_digest: None,
            agent_event_metadata_generation: 0,
            transcript_index: NativeTranscriptIndex {
                through_revision: event.revision,
                ..NativeTranscriptIndex::default()
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ThreadIndexMutation<'a> {
    #[default]
    Unchanged,
    Name(Option<&'a str>),
    Summary(Option<&'a str>),
    Archived(bool),
    Workspace(&'a str),
    Preview(Option<&'a str>),
    PreviewSnapshot(&'a Option<String>, &'a Option<String>),
    UserMessage(&'a str),
    Cost(Option<i64>),
    ResumeConfig(&'a Option<String>, &'a Option<String>, &'a Option<String>),
    NativeAgent {
        sequence: u64,
        turn_started: bool,
    },
    ModelContextCheckpoint(Option<ThreadRevision>),
    DynamicTools(&'a ContentRef),
    AgentEventMetadataGeneration(u32),
    AgentEventTimeline(u32, &'a Option<i64>, &'a Option<i64>),
}

impl<'a> ThreadIndexMutation<'a> {
    pub(crate) fn from_event(event: &'a ThreadEventEnvelope) -> Self {
        match &event.body {
            ThreadEventBody::ThreadNameSet { name } => Self::Name(name.as_deref()),
            ThreadEventBody::ThreadSummarySet { summary } => Self::Summary(summary.as_deref()),
            ThreadEventBody::ThreadArchived { archived } => Self::Archived(*archived),
            ThreadEventBody::ThreadWorkspaceSet { workspace } => Self::Workspace(workspace),
            ThreadEventBody::NativeAgentEventRecorded {
                agent_sequence,
                route,
                ..
            } => Self::NativeAgent {
                sequence: *agent_sequence,
                turn_started: matches!(
                    route,
                    praxis_thread_store_contracts::AgentEventRoute::TurnStarted
                ),
            },
            ThreadEventBody::ModelContextSnapshotReplaced { basis_revision, .. } => {
                Self::ModelContextCheckpoint(
                    (*basis_revision < event.revision).then_some(*basis_revision),
                )
            }
            ThreadEventBody::ModelContextRolledBack { .. } => Self::ModelContextCheckpoint(None),
            ThreadEventBody::ThreadDynamicToolsSet { tools } => Self::DynamicTools(tools),
            ThreadEventBody::ThreadPreviewSet {
                preview,
                first_user_message,
            } => Self::PreviewSnapshot(preview, first_user_message),
            ThreadEventBody::AgentEventMetadataReconciled { generation } => {
                Self::AgentEventMetadataGeneration(*generation)
            }
            ThreadEventBody::AgentEventTimelineReconciled {
                generation,
                created_at_unix_ms,
                updated_at_unix_ms,
            } => Self::AgentEventTimeline(*generation, created_at_unix_ms, updated_at_unix_ms),
            ThreadEventBody::TurnCostRecorded { cost_micros } => Self::Cost(*cost_micros),
            ThreadEventBody::ThreadResumeConfigSet {
                model,
                model_provider,
                reasoning_effort,
            } => Self::ResumeConfig(model, model_provider, reasoning_effort),
            ThreadEventBody::TranscriptItemCreated {
                item_kind: praxis_thread_store_contracts::TranscriptItemKind::UserMessage,
                content,
                ..
            } => content_preview(content).map_or(Self::Preview(None), Self::UserMessage),
            ThreadEventBody::TranscriptItemCreated { content, .. }
            | ThreadEventBody::TranscriptItemFinalized { content, .. } => {
                Self::Preview(content_preview(content))
            }
            ThreadEventBody::ContentRedacted { replacement, .. } => {
                Self::Preview(replacement.as_ref().and_then(content_preview))
            }
            _ => Self::Unchanged,
        }
    }

    fn apply(self, projection: &mut ThreadIndexProjection, revision: ThreadRevision) {
        match self {
            Self::Unchanged => {}
            Self::Name(value) => replace_optional_string(&mut projection.summary.name, value),
            Self::Summary(value) => replace_optional_string(&mut projection.summary.summary, value),
            Self::Archived(value) => projection.summary.archived = value,
            Self::Workspace(value) => replace_string(&mut projection.summary.workspace, value),
            Self::Preview(value) => replace_optional_string(&mut projection.summary.preview, value),
            Self::PreviewSnapshot(preview, first_user_message) => {
                replace_optional_string(&mut projection.summary.preview, preview.as_deref());
                replace_optional_string(
                    &mut projection.summary.first_user_message,
                    first_user_message.as_deref(),
                );
            }
            Self::UserMessage(value) => {
                replace_optional_string(&mut projection.summary.preview, Some(value));
                if projection.summary.first_user_message.is_none() {
                    projection.summary.first_user_message = Some(value.to_owned());
                }
            }
            Self::Cost(value) => {
                projection.summary.last_cost_micros = value;
                if let Some(value) = value {
                    projection.summary.total_cost_micros = Some(
                        projection
                            .summary
                            .total_cost_micros
                            .unwrap_or_default()
                            .saturating_add(value),
                    );
                }
            }
            Self::ResumeConfig(model, provider, effort) => {
                replace_optional_string(&mut projection.summary.model, model.as_deref());
                replace_optional_string(
                    &mut projection.summary.model_provider,
                    provider.as_deref(),
                );
                replace_optional_string(
                    &mut projection.summary.reasoning_effort,
                    effort.as_deref(),
                );
            }
            Self::NativeAgent {
                sequence,
                turn_started,
            } => {
                let previous =
                    (projection.last_agent_sequence != 0).then_some(projection.last_agent_sequence);
                if turn_started {
                    projection
                        .transcript_index
                        .native_turn_started(revision, previous);
                }
                projection.last_agent_sequence = projection.last_agent_sequence.max(sequence);
                projection.transcript_index.frontier_sequence =
                    Some(projection.last_agent_sequence);
            }
            Self::ModelContextCheckpoint(checkpoint) => {
                projection.model_context_checkpoint = checkpoint;
            }
            Self::DynamicTools(tools) => {
                projection.dynamic_tools_digest = Some(dynamic_tools_digest(tools));
            }
            Self::AgentEventMetadataGeneration(generation) => {
                projection.agent_event_metadata_generation = generation;
            }
            Self::AgentEventTimeline(generation, created_at, updated_at) => {
                projection.agent_event_metadata_generation = generation;
                if let Some(created_at) = created_at {
                    projection.summary.created_at_unix_ms = *created_at;
                }
                if let Some(updated_at) = updated_at {
                    projection.summary.updated_at_unix_ms = *updated_at;
                }
            }
        }
    }
}

impl ThreadIndexProjection {
    pub(crate) fn dynamic_tools_match(&self, tools: &ContentRef) -> bool {
        self.dynamic_tools_digest == Some(dynamic_tools_digest(tools))
    }
}

pub(crate) fn dynamic_tools_digest(tools: &ContentRef) -> Digest {
    tools.canonical_digest(DYNAMIC_TOOLS_DIGEST_DOMAIN)
}

fn replace_optional_string(target: &mut Option<String>, value: Option<&str>) {
    match value {
        Some(value) => replace_string(target.get_or_insert_with(String::new), value),
        None => *target = None,
    }
}

fn replace_string(target: &mut String, value: &str) {
    target.clear();
    target.push_str(value);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ThreadIndexAccumulator {
    AwaitingCreation,
    Projected(ThreadIndexProjection),
    Rejected,
}

impl Default for ThreadIndexAccumulator {
    fn default() -> Self {
        Self::AwaitingCreation
    }
}

impl ThreadIndexAccumulator {
    pub(crate) fn from_projection(projection: ThreadIndexProjection) -> Self {
        Self::Projected(projection)
    }

    pub(crate) const fn revision(&self) -> Option<ThreadRevision> {
        match self {
            Self::Projected(projection) => Some(projection.summary.revision),
            Self::AwaitingCreation | Self::Rejected => None,
        }
    }

    pub(crate) fn push(&mut self, event: &ThreadEventEnvelope) {
        if let Self::AwaitingCreation = self {
            *self =
                ThreadIndexProjection::from_created(event).map_or(Self::Rejected, Self::Projected);
        }
        let Self::Projected(projection) = self else {
            return;
        };
        if event.revision <= projection.summary.revision {
            return;
        }
        projection.summary.revision = event.revision;
        projection.summary.updated_at_unix_ms = event.recorded_at_unix_ms;
        projection.transcript_index.through_revision = event.revision;
        ThreadIndexMutation::from_event(event).apply(projection, event.revision);
    }

    pub(crate) fn finish(self) -> Option<ThreadIndexProjection> {
        match self {
            Self::Projected(projection) => Some(projection),
            Self::AwaitingCreation | Self::Rejected => None,
        }
    }
}

fn preview(text: &str) -> &str {
    const MAX_CHARS: usize = 160;
    text.char_indices()
        .nth(MAX_CHARS)
        .map_or(text, |(end, _)| &text[..end])
}

fn content_preview(content: &praxis_thread_store_contracts::ContentRef) -> Option<&str> {
    match content {
        praxis_thread_store_contracts::ContentRef::InlineText { text } => Some(preview(text)),
        praxis_thread_store_contracts::ContentRef::Artifact { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::ThreadIndexAccumulator;
    use super::ThreadIndexMutation;
    use super::preview;
    use praxis_thread_store_contracts::BatchId;
    use praxis_thread_store_contracts::ContentRef;
    use praxis_thread_store_contracts::Digest;
    use praxis_thread_store_contracts::EventId;
    use praxis_thread_store_contracts::ItemId;
    use praxis_thread_store_contracts::NewThreadEvent;
    use praxis_thread_store_contracts::ThreadActor;
    use praxis_thread_store_contracts::ThreadEventBody;
    use praxis_thread_store_contracts::ThreadEventEnvelope;
    use praxis_thread_store_contracts::ThreadId;
    use praxis_thread_store_contracts::ThreadRevision;

    #[test]
    fn index_projection_folds_summary_and_agent_sequence_incrementally() {
        let thread_id = ThreadId::new();
        let events = [
            event(
                thread_id,
                1,
                ThreadEventBody::ThreadCreated {
                    source: "run".into(),
                    workspace: "F:/Cunning3D".into(),
                    parent: None,
                },
            ),
            event(
                thread_id,
                2,
                ThreadEventBody::NativeAgentEventRecorded {
                    agent_sequence: 7,
                    event_id: "event-7".into(),
                    turn_id: None,
                    route: praxis_thread_store_contracts::AgentEventRoute::Other,
                    payload: praxis_thread_store_contracts::ContentRef::InlineText {
                        text: "{}".into(),
                    },
                },
            ),
            event(
                thread_id,
                3,
                ThreadEventBody::ThreadNameSet {
                    name: Some("Praxis".into()),
                },
            ),
        ];
        let mut accumulator = ThreadIndexAccumulator::default();
        for event in &events {
            accumulator.push(event);
        }

        let projection = accumulator.finish().expect("index projection");
        assert_eq!(projection.summary.name.as_deref(), Some("Praxis"));
        assert_eq!(projection.summary.revision, ThreadRevision::new(3));
        assert_eq!(projection.last_agent_sequence, 7);
    }

    #[test]
    fn indexed_projection_only_folds_the_recovery_suffix() {
        let thread_id = ThreadId::new();
        let mut initial = ThreadIndexAccumulator::default();
        initial.push(&event(
            thread_id,
            1,
            ThreadEventBody::ThreadCreated {
                source: "run".into(),
                workspace: "F:/Cunning3D".into(),
                parent: None,
            },
        ));
        initial.push(&event(
            thread_id,
            2,
            ThreadEventBody::ThreadNameSet {
                name: Some("indexed".into()),
            },
        ));
        let mut resumed =
            ThreadIndexAccumulator::from_projection(initial.finish().expect("indexed projection"));

        resumed.push(&event(
            thread_id,
            1,
            ThreadEventBody::ThreadCreated {
                source: "must-not-replace".into(),
                workspace: "must-not-replace".into(),
                parent: None,
            },
        ));
        resumed.push(&event(
            thread_id,
            2,
            ThreadEventBody::ThreadNameSet {
                name: Some("must-not-replace".into()),
            },
        ));
        resumed.push(&event(
            thread_id,
            3,
            ThreadEventBody::ThreadSummarySet {
                summary: Some("suffix".into()),
            },
        ));

        let projection = resumed.finish().expect("resumed projection");
        assert_eq!(projection.summary.revision, ThreadRevision::new(3));
        assert_eq!(projection.summary.name.as_deref(), Some("indexed"));
        assert_eq!(projection.summary.summary.as_deref(), Some("suffix"));
    }

    #[test]
    fn index_projection_requires_creation_as_the_first_event() {
        let thread_id = ThreadId::new();
        let mut accumulator = ThreadIndexAccumulator::default();
        accumulator.push(&event(
            thread_id,
            1,
            ThreadEventBody::ThreadNameSet {
                name: Some("invalid".into()),
            },
        ));
        accumulator.push(&event(
            thread_id,
            2,
            ThreadEventBody::ThreadCreated {
                source: "run".into(),
                workspace: "F:/Cunning3D".into(),
                parent: None,
            },
        ));
        assert!(accumulator.finish().is_none());
    }

    #[test]
    fn index_accumulator_has_one_explicit_state_authority() {
        let source = include_str!("projection.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("projection production source");
        let accumulator = source
            .split("pub(crate) enum ThreadIndexAccumulator")
            .nth(1)
            .expect("accumulator enum")
            .split("impl Default for ThreadIndexAccumulator")
            .next()
            .expect("accumulator states");
        assert!(accumulator.contains("AwaitingCreation"));
        assert!(accumulator.contains("Projected(ThreadIndexProjection)"));
        assert!(accumulator.contains("Rejected"));
        assert!(!source.contains("first_seen"));
        assert!(!source.contains("projection: Option<ThreadIndexProjection>"));
    }

    #[test]
    fn one_event_cannot_encode_multiple_index_mutations() {
        assert!(std::mem::size_of::<ThreadIndexMutation<'static>>() <= 24);
        let source = include_str!("projection.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("projection production source");
        let mutation = source
            .split("pub(crate) enum ThreadIndexMutation")
            .nth(1)
            .expect("mutation enum")
            .split("impl<'a> ThreadIndexMutation")
            .next()
            .expect("mutation variants");
        assert!(mutation.contains("Unchanged"));
        assert!(mutation.contains("UserMessage(&'a str)"));
        assert!(mutation.contains("NativeAgent"));
        assert!(!mutation.contains("LegacyAgent"));
        assert!(!source.contains("pub(crate) struct ThreadIndexMutation"));
        assert!(!source.contains("agent_sequence: Option<u64>"));
    }

    #[test]
    fn first_user_message_is_projected_once_without_a_second_mutation() {
        let thread_id = ThreadId::new();
        let mut accumulator = ThreadIndexAccumulator::default();
        accumulator.push(&event(
            thread_id,
            1,
            ThreadEventBody::ThreadCreated {
                source: "run".into(),
                workspace: "F:/Cunning3D".into(),
                parent: None,
            },
        ));
        for (revision, text) in [(2, "first ask"), (3, "later ask")] {
            accumulator.push(&event(
                thread_id,
                revision,
                ThreadEventBody::TranscriptItemCreated {
                    item_id: ItemId::new(),
                    turn_id: None,
                    item_kind: praxis_thread_store_contracts::TranscriptItemKind::UserMessage,
                    content: ContentRef::InlineText { text: text.into() },
                },
            ));
        }
        let summary = accumulator.finish().expect("projection").summary;
        assert_eq!(summary.first_user_message.as_deref(), Some("first ask"));
        assert_eq!(summary.preview.as_deref(), Some("later ask"));
    }

    #[test]
    fn reconciled_agent_metadata_replaces_preview_and_records_generation() {
        let thread_id = ThreadId::new();
        let mut accumulator = ThreadIndexAccumulator::default();
        for (revision, body) in [
            (
                1,
                ThreadEventBody::ThreadCreated {
                    source: "run".into(),
                    workspace: "F:/Cunning3D".into(),
                    parent: None,
                },
            ),
            (
                2,
                ThreadEventBody::ThreadPreviewSet {
                    preview: Some("latest".into()),
                    first_user_message: Some("first".into()),
                },
            ),
            (
                3,
                ThreadEventBody::AgentEventTimelineReconciled {
                    generation: 1,
                    created_at_unix_ms: Some(10),
                    updated_at_unix_ms: Some(20),
                },
            ),
        ] {
            accumulator.push(&event(thread_id, revision, body));
        }

        let projection = accumulator.finish().expect("reconciled projection");
        assert_eq!(projection.summary.preview.as_deref(), Some("latest"));
        assert_eq!(
            projection.summary.first_user_message.as_deref(),
            Some("first")
        );
        assert_eq!(projection.agent_event_metadata_generation, 1);
        assert_eq!(projection.summary.created_at_unix_ms, 10);
        assert_eq!(projection.summary.updated_at_unix_ms, 20);
    }

    #[test]
    fn turn_cost_projection_accumulates_known_cost_and_clears_unknown_last_cost() {
        let thread_id = ThreadId::new();
        let mut accumulator = ThreadIndexAccumulator::default();
        for (revision, body) in [
            (
                1,
                ThreadEventBody::ThreadCreated {
                    source: "run".into(),
                    workspace: "F:/Cunning3D".into(),
                    parent: None,
                },
            ),
            (
                2,
                ThreadEventBody::TurnCostRecorded {
                    cost_micros: Some(7),
                },
            ),
            (
                3,
                ThreadEventBody::TurnCostRecorded {
                    cost_micros: Some(5),
                },
            ),
            (4, ThreadEventBody::TurnCostRecorded { cost_micros: None }),
        ] {
            accumulator.push(&event(thread_id, revision, body));
        }
        let summary = accumulator.finish().expect("projection").summary;
        assert_eq!(summary.total_cost_micros, Some(12));
        assert_eq!(summary.last_cost_micros, None);
    }

    #[test]
    fn model_context_checkpoint_tracks_snapshots_and_rollbacks() {
        let thread_id = ThreadId::new();
        let mut accumulator = ThreadIndexAccumulator::default();
        accumulator.push(&event(
            thread_id,
            1,
            ThreadEventBody::ThreadCreated {
                source: "run".into(),
                workspace: "F:/Cunning3D".into(),
                parent: None,
            },
        ));
        accumulator.push(&event(
            thread_id,
            2,
            ThreadEventBody::ModelContextSnapshotReplaced {
                basis_revision: ThreadRevision::new(1),
                snapshot: ContentRef::InlineText {
                    text: "snapshot".into(),
                },
            },
        ));
        assert_eq!(
            accumulator
                .clone()
                .finish()
                .expect("snapshot projection")
                .model_context_checkpoint,
            Some(ThreadRevision::new(1))
        );

        accumulator.push(&event(
            thread_id,
            3,
            ThreadEventBody::ModelContextRolledBack { user_turns: 1 },
        ));
        assert_eq!(
            accumulator
                .finish()
                .expect("rollback projection")
                .model_context_checkpoint,
            None
        );
    }

    #[test]
    fn dynamic_tools_projection_tracks_the_latest_canonical_content() {
        let thread_id = ThreadId::new();
        let mut accumulator = ThreadIndexAccumulator::default();
        for (revision, body) in [
            (
                1,
                ThreadEventBody::ThreadCreated {
                    source: "run".into(),
                    workspace: "F:/Cunning3D".into(),
                    parent: None,
                },
            ),
            (
                2,
                ThreadEventBody::ThreadDynamicToolsSet {
                    tools: ContentRef::InlineText { text: "[]".into() },
                },
            ),
        ] {
            accumulator.push(&event(thread_id, revision, body));
        }
        let projection = accumulator.finish().expect("dynamic tools projection");
        assert!(projection.dynamic_tools_match(&ContentRef::InlineText { text: "[]".into() }));
        assert!(!projection.dynamic_tools_match(&ContentRef::InlineText {
            text: "[{}]".into(),
        }));
    }

    #[test]
    fn preview_borrows_a_utf8_safe_prefix() {
        let text = "界".repeat(161);
        let projected = preview(&text);
        assert_eq!(projected.chars().count(), 160);
        assert!(text.starts_with(projected));
    }

    #[test]
    fn unavailable_or_redacted_content_clears_the_index_preview() {
        let thread_id = ThreadId::new();
        let item_id = ItemId::new();
        let mut accumulator = ThreadIndexAccumulator::default();
        accumulator.push(&event(
            thread_id,
            1,
            ThreadEventBody::ThreadCreated {
                source: "run".into(),
                workspace: "F:/Cunning3D".into(),
                parent: None,
            },
        ));
        accumulator.push(&event(
            thread_id,
            2,
            ThreadEventBody::TranscriptItemCreated {
                item_id,
                turn_id: None,
                item_kind: praxis_thread_store_contracts::TranscriptItemKind::UserMessage,
                content: ContentRef::InlineText {
                    text: "private preview".into(),
                },
            },
        ));
        assert_eq!(
            accumulator
                .clone()
                .finish()
                .expect("inline projection")
                .summary
                .preview
                .as_deref(),
            Some("private preview"),
        );

        accumulator.push(&event(
            thread_id,
            3,
            ThreadEventBody::TranscriptItemFinalized {
                item_id,
                content: ContentRef::Artifact {
                    digest: Digest::ZERO,
                    bytes: 1,
                    media_type: "application/octet-stream".into(),
                },
            },
        ));
        assert_eq!(
            accumulator
                .clone()
                .finish()
                .expect("artifact projection")
                .summary
                .preview,
            None,
        );

        accumulator.push(&event(
            thread_id,
            4,
            ThreadEventBody::TranscriptItemFinalized {
                item_id,
                content: ContentRef::InlineText {
                    text: "replacement preview".into(),
                },
            },
        ));
        accumulator.push(&event(
            thread_id,
            5,
            ThreadEventBody::ContentRedacted {
                item_id,
                replacement: None,
                reason: "privacy".into(),
            },
        ));
        assert_eq!(
            accumulator
                .finish()
                .expect("redacted projection")
                .summary
                .preview,
            None,
        );
    }

    fn event(thread_id: ThreadId, revision: u64, body: ThreadEventBody) -> ThreadEventEnvelope {
        ThreadEventEnvelope::new(NewThreadEvent {
            thread_id,
            revision: ThreadRevision::new(revision),
            event_id: EventId::new(),
            batch_id: BatchId::new(),
            sequence: 0,
            recorded_at_unix_ms: revision as i64,
            actor: ThreadActor::Runtime,
            correlation_id: None,
            causation_id: None,
            body,
            previous_record_digest: praxis_thread_store_contracts::Digest::ZERO,
        })
    }
}

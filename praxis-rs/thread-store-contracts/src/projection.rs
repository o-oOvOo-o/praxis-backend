use crate::Digest;
use crate::ThreadId;
use crate::ThreadRevision;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectionId(String);

impl ProjectionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionPriority {
    Critical,
    Interactive,
    Background,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismClass {
    Deterministic,
    ExternalObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildBehavior {
    Rebuildable,
    RequiresExternalSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaRange {
    pub min_inclusive: u32,
    pub max_inclusive: u32,
}

impl SchemaRange {
    pub const fn includes(self, version: u32) -> bool {
        version >= self.min_inclusive && version <= self.max_inclusive
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionDescriptor {
    pub id: ProjectionId,
    pub schema_version: u32,
    pub consumed_event_types: Vec<String>,
    pub dependencies: Vec<ProjectionId>,
    pub priority: ProjectionPriority,
    pub determinism: DeterminismClass,
    pub rebuild: RebuildBehavior,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCheckpoint {
    pub projection_id: ProjectionId,
    pub thread_id: ThreadId,
    pub schema_version: u32,
    pub through_revision: ThreadRevision,
    pub input_digest: Digest,
    pub output_digest: Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "revision", rename_all = "snake_case")]
pub enum ReadConsistency {
    Eventual,
    AtLeast(ThreadRevision),
    Head,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    JournalBackend,
    EventCodec,
    EventMigration,
    Projector,
    Indexer,
    SnapshotBackend,
    CompactionPolicy,
    Importer,
    Exporter,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadStorePluginDescriptor {
    pub plugin_id: String,
    pub plugin_version: String,
    pub contract_versions: SchemaRange,
    pub event_schema_versions: SchemaRange,
    pub capabilities: Vec<PluginCapability>,
    pub projections: Vec<ProjectionDescriptor>,
}

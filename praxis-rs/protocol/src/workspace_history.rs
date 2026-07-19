use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
#[serde(transparent)]
#[ts(type = "string")]
pub struct WorkspaceCheckpointId(pub Uuid);

impl WorkspaceCheckpointId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkspaceCheckpointId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkspaceCheckpointId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceMutationKind {
    Add,
    Update,
    Delete,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCheckpointRef {
    pub id: WorkspaceCheckpointId,
    pub workspace_root: PathBuf,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub created_at_unix_ms: i64,
    pub changed_file_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCheckpointFileSummary {
    pub path: PathBuf,
    pub previous_path: Option<PathBuf>,
    pub kind: WorkspaceMutationKind,
    pub byte_size: u64,
}

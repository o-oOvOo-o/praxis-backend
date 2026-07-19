use std::path::PathBuf;

use praxis_protocol::workspace_history::WorkspaceCheckpointId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFileVersion {
    pub path: PathBuf,
    pub blob_hash: String,
    pub byte_size: u64,
    #[serde(default)]
    pub modified_at_unix_ns: u128,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCheckpointManifest {
    pub schema_version: u32,
    pub id: WorkspaceCheckpointId,
    pub workspace_root: PathBuf,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub operation_id: Option<String>,
    pub created_at_unix_ms: i64,
    pub files: Vec<WorkspaceFileVersion>,
    pub skipped_files: Vec<PathBuf>,
}

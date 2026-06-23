use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ArtifactRecord {
    pub(crate) artifact_id: String,
    pub(crate) task_id: String,
    pub(crate) owner_thread_id: ThreadId,
    pub(crate) artifact_type: ArtifactType,
    pub(crate) uri: String,
    pub(crate) summary: String,
    pub(crate) metadata: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ArtifactBlobRead {
    pub(crate) artifact: ArtifactRecord,
    pub(crate) content: String,
    pub(crate) bytes_read: usize,
    pub(crate) blob_bytes: Option<u64>,
    pub(crate) truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ArtifactType {
    CommandLog,
    CompileLog,
    DirtyFileReport,
    DecisionRecord,
    PatchMetadata,
}

use crate::agent_os::records::ArtifactRecord;
use crate::util::truncate_to_char_boundary;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsArtifactSummary {
    artifact_id: String,
    task_id: String,
    owner_thread_id: String,
    artifact_type: String,
    uri: String,
    summary: String,
    blob_persisted: bool,
    blob_bytes: Option<u64>,
    blob_path: Option<String>,
    created_at: String,
}

impl From<ArtifactRecord> for AgentOsArtifactSummary {
    fn from(artifact: ArtifactRecord) -> Self {
        let mut summary = artifact.summary;
        truncate_to_char_boundary(&mut summary, 500);
        let blob = artifact.metadata.get("blob");
        Self {
            artifact_id: artifact.artifact_id,
            task_id: artifact.task_id,
            owner_thread_id: artifact.owner_thread_id.to_string(),
            artifact_type: format!("{:?}", artifact.artifact_type),
            uri: artifact.uri,
            summary,
            blob_persisted: blob
                .and_then(|value| value.get("blob_persisted"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            blob_bytes: blob
                .and_then(|value| value.get("blob_bytes"))
                .and_then(|value| value.as_u64()),
            blob_path: blob
                .and_then(|value| value.get("blob_path"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
            created_at: artifact.created_at.to_rfc3339(),
        }
    }
}

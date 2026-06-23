use super::*;

mod authorization;
mod blob_file;

use blob_file::artifact_blob_metadata;

impl AgentOs {
    pub(crate) async fn read_artifact_blob(
        &self,
        reader_thread_id: ThreadId,
        artifact_id: &str,
        max_bytes: Option<usize>,
    ) -> PraxisResult<ArtifactBlobRead> {
        let requested_max_bytes = max_bytes
            .unwrap_or_else(|| AgentOsPolicy::get().default_artifact_read_max_bytes)
            .clamp(1, HARD_ARTIFACT_READ_MAX_BYTES);
        let (artifact, reader_task_id, max_bytes) = self
            .authorize_artifact_blob_read(reader_thread_id, artifact_id, requested_max_bytes)
            .await?;
        let blob = artifact_blob_metadata(artifact_id, &artifact)?;
        let bytes = self
            .read_artifact_blob_bytes(artifact_id, blob.path.as_str(), max_bytes)
            .await?;
        let truncated = blob.truncated_by(bytes.len(), max_bytes);
        let content = String::from_utf8_lossy(&bytes).to_string();
        self.record_artifact_read_budget(reader_task_id.as_str(), bytes.len() as u64)
            .await?;
        self.record_event(
            "artifact_blob_read",
            Some(reader_thread_id),
            Some(reader_task_id.clone()),
            None,
            json!({
                "artifact_id": &artifact.artifact_id,
                "artifact_owner_thread_id": artifact.owner_thread_id.to_string(),
                "artifact_task_id": &artifact.task_id,
                "bytes_read": bytes.len(),
                "blob_bytes": blob.bytes,
                "truncated": truncated,
            }),
        )
        .await;
        Ok(ArtifactBlobRead {
            artifact,
            content,
            bytes_read: bytes.len(),
            blob_bytes: blob.bytes,
            truncated,
        })
    }
}

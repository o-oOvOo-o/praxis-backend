use super::super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn create_blob_artifact(
        &self,
        task_id: String,
        owner_thread_id: ThreadId,
        artifact_type: ArtifactType,
        uri_namespace: &str,
        summary: String,
        metadata: serde_json::Value,
        extension: &str,
        blob: &[u8],
    ) -> PraxisResult<String> {
        let artifact_id = format!("artifact-{}", Uuid::new_v4());
        let blob_path = self
            .write_artifact_blob(artifact_id.as_str(), extension, blob)
            .await;
        let artifact = ArtifactRecord {
            artifact_id: artifact_id.clone(),
            task_id,
            owner_thread_id,
            artifact_type,
            uri: format!("artifact://{uri_namespace}/{artifact_id}"),
            summary,
            metadata: metadata_with_blob(metadata, blob.len(), blob_path.as_ref()),
            created_at: Utc::now(),
        };
        self.insert_artifact_record(artifact).await
    }

    pub(in crate::agent_os) async fn create_blob_artifact_from_spool(
        &self,
        task_id: String,
        owner_thread_id: ThreadId,
        artifact_type: ArtifactType,
        uri_namespace: &str,
        summary: String,
        metadata: serde_json::Value,
        extension: &str,
        spool: &ExecOutputSpool,
    ) -> PraxisResult<String> {
        let artifact_id = format!("artifact-{}", Uuid::new_v4());
        let blob_path = self
            .write_artifact_blob_from_spool(artifact_id.as_str(), extension, spool)
            .await;
        let artifact = ArtifactRecord {
            artifact_id: artifact_id.clone(),
            task_id,
            owner_thread_id,
            artifact_type,
            uri: format!("artifact://{uri_namespace}/{artifact_id}"),
            summary,
            metadata: metadata_with_blob(metadata, spool.total_bytes(), blob_path.as_ref()),
            created_at: Utc::now(),
        };
        self.insert_artifact_record(artifact).await
    }
}

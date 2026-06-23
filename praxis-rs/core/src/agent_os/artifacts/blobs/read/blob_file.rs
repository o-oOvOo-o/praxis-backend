use super::super::*;

pub(super) struct ArtifactBlobMetadata {
    pub(super) path: String,
    pub(super) bytes: Option<u64>,
}

impl ArtifactBlobMetadata {
    pub(super) fn truncated_by(&self, bytes_read: usize, max_bytes: usize) -> bool {
        self.bytes
            .map(|total| total > bytes_read as u64)
            .unwrap_or(bytes_read == max_bytes)
    }
}

pub(super) fn artifact_blob_metadata(
    artifact_id: &str,
    artifact: &ArtifactRecord,
) -> PraxisResult<ArtifactBlobMetadata> {
    let blob = artifact.metadata.get("blob").ok_or_else(|| {
        PraxisErr::UnsupportedOperation(format!("artifact `{artifact_id}` has no blob metadata"))
    })?;
    let path = blob
        .get("blob_path")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!("artifact `{artifact_id}` has no blob path"))
        })?
        .to_string();
    let bytes = blob.get("blob_bytes").and_then(|value| value.as_u64());
    Ok(ArtifactBlobMetadata { path, bytes })
}

impl AgentOs {
    pub(in crate::agent_os::artifacts::blobs::read) async fn read_artifact_blob_bytes(
        &self,
        artifact_id: &str,
        blob_path: &str,
        max_bytes: usize,
    ) -> PraxisResult<Vec<u8>> {
        let path = self.validated_artifact_blob_path(blob_path).await?;
        let file = tokio::fs::File::open(path.as_path()).await.map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to open artifact `{artifact_id}` blob: {err}"
            ))
        })?;
        let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
        let mut limited_file = file.take(max_bytes as u64);
        limited_file.read_to_end(&mut bytes).await.map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to read artifact `{artifact_id}` blob: {err}"
            ))
        })?;
        Ok(bytes)
    }
}

use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn write_artifact_blob(
        &self,
        artifact_id: &str,
        extension: &str,
        blob: &[u8],
    ) -> Option<PathBuf> {
        let root = self.artifact_blob_root().await?;
        if let Err(err) = tokio::fs::create_dir_all(root.as_path()).await {
            tracing::warn!("failed to create AgentOS artifact directory: {err}");
            return None;
        }
        let extension = sanitize_artifact_extension(extension);
        let path = root.join(format!("{artifact_id}.{extension}"));
        if let Err(err) = tokio::fs::write(path.as_path(), blob).await {
            tracing::warn!("failed to write AgentOS artifact blob: {err}");
            return None;
        }
        Some(path)
    }

    pub(in crate::agent_os) async fn write_artifact_blob_from_spool(
        &self,
        artifact_id: &str,
        extension: &str,
        spool: &ExecOutputSpool,
    ) -> Option<PathBuf> {
        let root = self.artifact_blob_root().await?;
        if let Err(err) = tokio::fs::create_dir_all(root.as_path()).await {
            tracing::warn!("failed to create AgentOS artifact directory: {err}");
            return None;
        }
        let extension = sanitize_artifact_extension(extension);
        let path = root.join(format!("{artifact_id}.{extension}"));
        let mut out = match tokio::fs::File::create(path.as_path()).await {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!("failed to create AgentOS artifact blob: {err}");
                return None;
            }
        };
        for stream in [&spool.stdout, &spool.stderr].into_iter().flatten() {
            if let Err(err) = append_spool_stream(&mut out, stream).await {
                tracing::warn!("failed to persist AgentOS artifact spool: {err}");
                let _ = tokio::fs::remove_file(path.as_path()).await;
                return None;
            }
        }
        if let Err(err) = out.flush().await {
            tracing::warn!("failed to flush AgentOS artifact blob: {err}");
            let _ = tokio::fs::remove_file(path.as_path()).await;
            return None;
        }
        Some(path)
    }

    pub(in crate::agent_os) async fn validated_artifact_blob_path(
        &self,
        blob_path: &str,
    ) -> PraxisResult<PathBuf> {
        let root = self.artifact_blob_root_or_err().await?;
        let root = std::fs::canonicalize(root.as_path()).map_err(|err| {
            PraxisErr::UnsupportedOperation(format!(
                "failed to resolve AgentOS artifact root: {err}"
            ))
        })?;
        let path = PathBuf::from(blob_path);
        let path = std::fs::canonicalize(path.as_path()).map_err(|err| {
            PraxisErr::UnsupportedOperation(format!("failed to resolve artifact blob path: {err}"))
        })?;
        if !path.starts_with(root.as_path()) {
            return Err(PraxisErr::UnsupportedOperation(
                "artifact blob path escapes AgentOS artifact root".to_string(),
            ));
        }
        Ok(path)
    }

    async fn artifact_blob_root(&self) -> Option<PathBuf> {
        Some(
            self.state_db
                .read()
                .await
                .clone()?
                .praxis_home()
                .join("artifacts")
                .join("agent-os"),
        )
    }

    async fn artifact_blob_root_or_err(&self) -> PraxisResult<PathBuf> {
        self.artifact_blob_root().await.ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "AgentOS artifact blob store is unavailable without state DB".to_string(),
            )
        })
    }
}

use super::*;

impl AgentOsRuntime {
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
        let blob = artifact.metadata.get("blob").ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "artifact `{artifact_id}` has no blob metadata"
            ))
        })?;
        let blob_path = blob
            .get("blob_path")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "artifact `{artifact_id}` has no blob path"
                ))
            })?;
        let blob_bytes = blob.get("blob_bytes").and_then(|value| value.as_u64());
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
        let truncated = blob_bytes
            .map(|total| total > bytes.len() as u64)
            .unwrap_or(bytes.len() == max_bytes);
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
                "blob_bytes": blob_bytes,
                "truncated": truncated,
            }),
        )
        .await;
        Ok(ArtifactBlobRead {
            artifact,
            content,
            bytes_read: bytes.len(),
            blob_bytes,
            truncated,
        })
    }

    pub(super) async fn authorize_artifact_blob_read(
        &self,
        reader_thread_id: ThreadId,
        artifact_id: &str,
        requested_max_bytes: usize,
    ) -> PraxisResult<(ArtifactRecord, String, usize)> {
        let state = self.state.read().await;
        let reader = state.threads.get(&reader_thread_id).ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS reader thread `{reader_thread_id}`"
            ))
        })?;
        let reader_task_id = reader.current_task_id.clone().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "artifact read rejected: reader thread has no current task_id".to_string(),
            )
        })?;
        let reader_task = state.tasks.get(&reader_task_id).ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "artifact read rejected: current task `{reader_task_id}` is not registered"
            ))
        })?;
        let artifact = state.artifacts.get(artifact_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!("unknown artifact `{artifact_id}`"))
        })?;
        let owner_scope_matches = state
            .threads
            .get(&artifact.owner_thread_id)
            .is_some_and(|owner| owner.coordination_scope == reader.coordination_scope);
        if !owner_scope_matches && artifact.owner_thread_id != reader_thread_id {
            return Err(PraxisErr::UnsupportedOperation(
                "artifact read rejected: artifact owner is outside reader coordination scope"
                    .to_string(),
            ));
        }
        let artifact_ref_allowed = reader_task
            .artifact_refs
            .iter()
            .any(|reference| reference == artifact_id || reference == &artifact.uri);
        let same_task = artifact.task_id == reader_task_id;
        let coordinator = reader.rank == COORDINATOR_RANK;
        if !same_task && !artifact_ref_allowed && !reader_task.exploratory && !coordinator {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "artifact read rejected: artifact `{artifact_id}` is not in task artifact_refs"
            )));
        }
        let max_bytes = if let Some(token_budget) = reader_task.token_budget {
            let remaining = token_budget.saturating_sub(reader_task.artifact_read_bytes);
            if remaining == 0 {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "artifact read rejected: task `{reader_task_id}` token budget is exhausted"
                )));
            }
            requested_max_bytes.min(remaining as usize)
        } else {
            requested_max_bytes
        };
        Ok((artifact, reader_task_id, max_bytes.max(1)))
    }

    pub(super) async fn record_artifact_read_budget(
        &self,
        task_id: &str,
        bytes_read: u64,
    ) -> PraxisResult<()> {
        if bytes_read == 0 {
            return Ok(());
        }
        let task_snapshot = {
            let mut state = self.state.write().await;
            let task = state.tasks.get_mut(task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "artifact read budget rejected: task `{task_id}` is not registered"
                ))
            })?;
            task.artifact_read_bytes = task.artifact_read_bytes.saturating_add(bytes_read);
            task.updated_at = Utc::now();
            task.clone()
        };
        self.persist_task_snapshot(&task_snapshot).await;
        Ok(())
    }

    pub(super) async fn create_command_output_artifact(
        &self,
        command: &CommandRecord,
        command_id: &str,
        exit_code: Option<i32>,
        output_source: &ManagedCommandOutputSource<'_>,
    ) -> PraxisResult<String> {
        let metadata = json!({
            "command_id": command_id,
            "bytes": output_source.byte_len(),
            "exit_code": exit_code,
            "runtime_kind": command.runtime_kind.as_deref(),
            "runtime_owner_id": command.runtime_owner_id.as_deref(),
            "process_id": command.process_id,
        });
        match output_source {
            ManagedCommandOutputSource::Bytes(raw_output) => {
                self.create_blob_artifact(
                    command.task_id.clone(),
                    command.thread_id,
                    artifact_type_for_intent(command.intent),
                    "command-log",
                    output_source.summary(),
                    metadata,
                    "log",
                    raw_output,
                )
                .await
            }
            ManagedCommandOutputSource::Spool { spool, .. } => {
                self.create_blob_artifact_from_spool(
                    command.task_id.clone(),
                    command.thread_id,
                    artifact_type_for_intent(command.intent),
                    "command-log",
                    output_source.summary(),
                    metadata,
                    "log",
                    spool,
                )
                .await
            }
        }
    }

    pub(super) async fn create_blob_artifact(
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

    pub(super) async fn create_blob_artifact_from_spool(
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

    pub(super) async fn insert_artifact_record(
        &self,
        artifact: ArtifactRecord,
    ) -> PraxisResult<String> {
        let artifact_id = artifact.artifact_id.clone();
        {
            let mut state = self.state.write().await;
            state
                .artifacts
                .insert(artifact_id.clone(), artifact.clone());
        }
        self.persist_artifact_snapshot(&artifact).await;
        self.record_event(
            "artifact_created",
            Some(artifact.owner_thread_id),
            Some(artifact.task_id.clone()),
            None,
            json!({
                "artifact_id": artifact.artifact_id,
                "type": format!("{:?}", artifact.artifact_type),
                "uri": artifact.uri,
            }),
        )
        .await;
        Ok(artifact_id)
    }

    pub(super) async fn write_artifact_blob(
        &self,
        artifact_id: &str,
        extension: &str,
        blob: &[u8],
    ) -> Option<PathBuf> {
        let db = self.state_db.read().await.clone()?;
        let root = db.praxis_home().join("artifacts").join("agent-os");
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

    pub(super) async fn write_artifact_blob_from_spool(
        &self,
        artifact_id: &str,
        extension: &str,
        spool: &ExecOutputSpool,
    ) -> Option<PathBuf> {
        let db = self.state_db.read().await.clone()?;
        let root = db.praxis_home().join("artifacts").join("agent-os");
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

    pub(super) async fn validated_artifact_blob_path(
        &self,
        blob_path: &str,
    ) -> PraxisResult<PathBuf> {
        let db = self.state_db.read().await.clone().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(
                "AgentOS artifact blob store is unavailable without state DB".to_string(),
            )
        })?;
        let root = db.praxis_home().join("artifacts").join("agent-os");
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
}

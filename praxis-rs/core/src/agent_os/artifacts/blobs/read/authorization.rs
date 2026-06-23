use super::super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn authorize_artifact_blob_read(
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
}

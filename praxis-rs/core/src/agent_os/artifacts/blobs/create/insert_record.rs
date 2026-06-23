use super::super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn insert_artifact_record(
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
}

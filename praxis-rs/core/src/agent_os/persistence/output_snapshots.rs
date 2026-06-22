use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn persist_artifact_snapshot(&self, artifact: &ArtifactRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(artifact) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_artifact_snapshot(artifact.artifact_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS artifact snapshot: {err}");
        }
    }

    pub(in crate::agent_os) async fn persist_worker_request_snapshot(
        &self,
        request: &WorkerRequestRecord,
    ) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(request) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_worker_request_snapshot(request.request_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS worker request snapshot: {err}");
        }
    }
}

use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn record_artifact_read_budget(
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
}

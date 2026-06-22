use super::*;

impl AgentOs {
    pub(crate) async fn ensure_bootstrap_task(
        &self,
        thread_id: ThreadId,
        objective: impl Into<String>,
        scope: Vec<String>,
    ) -> PraxisResult<String> {
        if let Some(task_id) = self
            .state
            .read()
            .await
            .threads
            .get(&thread_id)
            .and_then(|thread| thread.current_task_id.clone())
        {
            return Ok(task_id);
        }
        let task = self
            .create_task(TaskCreateRequest {
                objective: objective.into(),
                scope,
                constraints: Vec::new(),
                acceptance_criteria: Vec::new(),
                artifact_refs: Vec::new(),
                priority: 0,
                assigned_thread_id: Some(thread_id),
                required_capabilities: Vec::new(),
                required_resources: Vec::new(),
                token_budget: None,
                exploratory: true,
                created_by: thread_id,
            })
            .await?;
        self.assign_task(task.as_str(), thread_id).await?;
        Ok(task)
    }
}

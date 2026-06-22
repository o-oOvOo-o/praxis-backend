use super::*;

impl AgentOs {
    pub(crate) async fn create_task(&self, request: TaskCreateRequest) -> PraxisResult<String> {
        let now = Utc::now();
        let task_id = format!("task-{}", Uuid::new_v4());
        let assigned_thread_id = request.assigned_thread_id;
        if assigned_thread_id.is_some() && request.scope.is_empty() && !request.exploratory {
            return Err(PraxisErr::UnsupportedOperation(
                "assigned AgentOS tasks require non-empty scope unless exploratory=true"
                    .to_string(),
            ));
        }
        let task = TaskRecord {
            task_id: task_id.clone(),
            objective: request.objective,
            scope: request.scope,
            constraints: request.constraints,
            acceptance_criteria: request.acceptance_criteria,
            artifact_refs: request.artifact_refs,
            status: if assigned_thread_id.is_some() {
                TaskStatus::Assigned
            } else {
                TaskStatus::Pending
            },
            priority: request.priority,
            assigned_thread_id,
            required_capabilities: request.required_capabilities,
            required_resources: request.required_resources,
            token_budget: request.token_budget,
            artifact_read_bytes: 0,
            exploratory: request.exploratory,
            created_by: request.created_by,
            created_at: now,
            updated_at: now,
        };

        {
            let mut state = self.state.write().await;
            state.tasks.insert(task_id.clone(), task.clone());
        }
        self.persist_task_snapshot(&task).await;
        self.record_event(
            "task_created",
            assigned_thread_id,
            Some(task_id.clone()),
            None,
            json!({
                "objective": task.objective,
                "scope": task.scope,
                "constraints": task.constraints,
                "acceptance_criteria": task.acceptance_criteria,
                "artifact_refs": task.artifact_refs,
                "priority": task.priority,
                "exploratory": task.exploratory,
            }),
        )
        .await;
        Ok(task_id)
    }

    pub(crate) async fn assign_task(&self, task_id: &str, thread_id: ThreadId) -> PraxisResult<()> {
        let (thread_snapshot, task_snapshot) = {
            let mut state = self.state.write().await;
            let task = state.tasks.get_mut(task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown task `{task_id}`"))
            })?;
            task.assigned_thread_id = Some(thread_id);
            task.status = TaskStatus::Assigned;
            task.updated_at = Utc::now();
            let task_snapshot = task.clone();
            let thread = state.threads.get_mut(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            thread.current_task_id = Some(task_id.to_string());
            thread.state = ThreadRuntimeState::Assigned;
            thread.heartbeat_at = Utc::now();
            (thread.clone(), task_snapshot)
        };

        self.persist_thread_snapshot(&thread_snapshot).await;
        self.persist_task_snapshot(&task_snapshot).await;
        self.record_event(
            "task_assigned",
            Some(thread_id),
            Some(task_id.to_string()),
            None,
            json!({ "thread_id": thread_id.to_string() }),
        )
        .await;
        Ok(())
    }
}

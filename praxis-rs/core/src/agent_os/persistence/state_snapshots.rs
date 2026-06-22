use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn persist_thread_snapshot(&self, entry: &ThreadRegistryEntry) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(entry) else {
            return;
        };
        let thread_id = entry.thread_id.to_string();
        if let Err(err) = db
            .upsert_agent_os_thread_snapshot(thread_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS thread snapshot: {err}");
        }
    }

    pub(in crate::agent_os) async fn persist_task_snapshot(&self, task: &TaskRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(task) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_task_snapshot(task.task_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS task snapshot: {err}");
        }
    }

    pub(in crate::agent_os) async fn persist_lease_snapshot(&self, lease: &ResourceLease) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(lease) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_lease_snapshot(lease.lease_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS lease snapshot: {err}");
        }
    }

    pub(in crate::agent_os) async fn persist_intent_plan_snapshot(&self, plan: &CommandIntentPlan) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(plan) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_intent_plan_snapshot(plan.plan_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS intent plan snapshot: {err}");
        }
    }
}

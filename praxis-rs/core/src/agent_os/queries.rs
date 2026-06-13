use super::*;

impl AgentOsRuntime {
    pub(crate) async fn query_leases(&self) -> Vec<ResourceLease> {
        self.expire_tickets().await;
        self.expire_leases().await;
        self.state.read().await.leases.values().cloned().collect()
    }

    pub(crate) async fn query_artifacts(&self) -> Vec<ArtifactRecord> {
        self.state
            .read()
            .await
            .artifacts
            .values()
            .cloned()
            .collect()
    }

    pub(crate) async fn query_worker_requests(&self) -> Vec<WorkerRequestRecord> {
        self.state
            .read()
            .await
            .worker_requests
            .values()
            .cloned()
            .collect()
    }

    pub(crate) async fn query_runtime_commands(&self) -> Vec<RuntimeCommandRecord> {
        self.expire_runtime_commands().await;
        self.state
            .read()
            .await
            .runtime_commands
            .values()
            .cloned()
            .collect()
    }

    pub(crate) async fn query_intent_plans(&self) -> Vec<CommandIntentPlan> {
        self.expire_intent_plans().await;
        self.state
            .read()
            .await
            .intent_plans
            .values()
            .cloned()
            .collect()
    }
}

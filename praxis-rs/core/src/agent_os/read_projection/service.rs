use crate::agent_os::instance::AgentOs;
use crate::agent_os::read_projection::AgentOsEventBatch;
use crate::agent_os::read_projection::AgentOsEventQuery;
use crate::agent_os::read_projection::AgentOsSnapshot;
use crate::agent_os::read_projection::AgentOsSnapshotOptions;

impl AgentOs {
    pub(crate) async fn events_since(&self, query: AgentOsEventQuery) -> AgentOsEventBatch {
        let state = self.state.read().await;
        AgentOsEventBatch::from_state(&state, query, || self.change_sequence())
    }

    pub(crate) async fn snapshot(&self, options: AgentOsSnapshotOptions) -> AgentOsSnapshot {
        self.expire_tickets().await;
        self.expire_leases().await;
        self.expire_runtime_commands().await;
        self.expire_intent_plans().await;

        let state = self.state.read().await;
        AgentOsSnapshot::from_state(&state, options, || self.change_sequence())
    }
}

use super::options::AgentOsEventQuery;
use crate::agent_os::model::EventLedgerEntry;
use crate::agent_os::state::AgentOsState;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsEventBatch {
    pub(crate) current_sequence: u64,
    pub(crate) events: Vec<EventLedgerEntry>,
}

impl AgentOsEventBatch {
    pub(in crate::agent_os) fn from_state(
        state: &AgentOsState,
        query: AgentOsEventQuery,
        current_sequence: impl FnOnce() -> u64,
    ) -> Self {
        let mut events = state
            .events
            .iter()
            .filter(|event| event.sequence > query.since_sequence)
            .cloned()
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.sequence);
        if events.len() > query.limit {
            let drop_count = events.len() - query.limit;
            events.drain(0..drop_count);
        }
        Self {
            current_sequence: current_sequence(),
            events,
        }
    }
}

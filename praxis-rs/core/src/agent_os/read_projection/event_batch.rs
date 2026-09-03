use super::options::AgentOsEventQuery;
use crate::agent_os::records::EventLedgerEntry;
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
            .rev()
            .take_while(|event| event.sequence > query.since_sequence)
            .take(query.limit)
            .cloned()
            .collect::<Vec<_>>();
        events.reverse();
        Self {
            current_sequence: current_sequence(),
            events,
        }
    }
}

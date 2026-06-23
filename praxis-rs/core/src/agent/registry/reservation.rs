use std::sync::Arc;
use std::sync::atomic::Ordering;

use praxis_protocol::AgentPath;

use crate::error::PraxisErr;
use crate::error::Result;

use super::AgentMetadata;
use super::AgentRegistry;

pub(crate) struct SpawnReservation {
    pub(super) state: Arc<AgentRegistry>,
    pub(super) active: bool,
    pub(super) reserved_agent_base_name: Option<String>,
    pub(super) reserved_agent_path: Option<AgentPath>,
}

impl SpawnReservation {
    pub(crate) fn reserve_agent_base_name_with_preference(
        &mut self,
        names: &[&str],
        preferred: Option<&str>,
    ) -> Result<String> {
        let agent_base_name = self
            .state
            .reserve_agent_base_name(names, preferred)
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation("no available agent base names".to_string())
            })?;
        self.reserved_agent_base_name = Some(agent_base_name.clone());
        Ok(agent_base_name)
    }

    pub(crate) fn reserve_agent_path(&mut self, agent_path: &AgentPath) -> Result<()> {
        self.state.reserve_agent_path(agent_path)?;
        self.reserved_agent_path = Some(agent_path.clone());
        Ok(())
    }

    pub(crate) fn commit(mut self, agent_metadata: AgentMetadata) {
        self.reserved_agent_base_name = None;
        self.reserved_agent_path = None;
        self.state.register_spawned_thread(agent_metadata);
        self.active = false;
    }
}

impl Drop for SpawnReservation {
    fn drop(&mut self) {
        if self.active {
            if let Some(agent_path) = self.reserved_agent_path.take() {
                self.state.release_reserved_agent_path(&agent_path);
            }
            self.state.total_count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

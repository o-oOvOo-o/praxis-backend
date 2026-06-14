use super::*;

impl AgentOs {
    pub(super) fn claim_or_renew_active_coordinator_locked(
        state: &mut AgentOsState,
        thread: &ThreadRegistryEntry,
        now: DateTime<Utc>,
        active_action: Option<&str>,
    ) -> PraxisResult<Option<ActiveCoordinatorLease>> {
        if thread.rank != COORDINATOR_RANK {
            if let Some(active_action) = active_action {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "only rank-0 coordinators can {active_action}"
                )));
            }
            return Ok(None);
        }

        let scope = thread.coordination_scope.clone();
        if let Some(active) = state.active_coordinators.get_mut(scope.as_str()) {
            if active.owner_thread_id == thread.thread_id {
                active.expires_at = now + AgentOsPolicy::get().lease_ttl();
                return Ok(Some(active.clone()));
            }
            if active.expires_at > now {
                if let Some(active_action) = active_action {
                    return Err(PraxisErr::UnsupportedOperation(format!(
                        "only the active rank-0 coordinator can {active_action}"
                    )));
                }
                return Ok(None);
            }
        }

        state.coordinator_epoch = state.coordinator_epoch.saturating_add(1);
        state.fencing_counter = state.fencing_counter.saturating_add(1);
        let active = ActiveCoordinatorLease {
            coordination_scope: scope.clone(),
            owner_thread_id: thread.thread_id,
            epoch: state.coordinator_epoch,
            fencing_token: state.fencing_counter,
            expires_at: now + AgentOsPolicy::get().lease_ttl(),
        };
        state.active_coordinators.insert(scope, active.clone());
        Ok(Some(active))
    }
}

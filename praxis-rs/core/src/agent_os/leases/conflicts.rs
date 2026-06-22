use super::*;

impl AgentOs {
    pub(in crate::agent_os) fn lease_conflict_owner_locked(
        &self,
        state: &AgentOsState,
        requirement: &ResourceRequirement,
    ) -> Option<String> {
        let key = requirement.key();
        match requirement.mode() {
            LeaseMode::Advisory | LeaseMode::Shared => None,
            LeaseMode::Capacity => capacity_conflict_owner(state, requirement, key),
            LeaseMode::Exclusive => exclusive_conflict_owner(state, key),
        }
    }
}

fn capacity_conflict_owner(
    state: &AgentOsState,
    requirement: &ResourceRequirement,
    key: String,
) -> Option<String> {
    let capacity = capacity_for_requirement(requirement);
    let active = state
        .leases
        .values()
        .filter(|lease| lease.scope == key)
        .filter(|lease| {
            lease
                .expires_at
                .is_none_or(|expires_at| expires_at > Utc::now())
        })
        .collect::<Vec<_>>();
    (active.len() >= capacity).then(|| {
        active
            .first()
            .map(|lease| lease.owner_thread_id.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    })
}

fn exclusive_conflict_owner(state: &AgentOsState, key: String) -> Option<String> {
    state
        .leases
        .values()
        .find(|lease| {
            lease.scope == key
                && lease
                    .expires_at
                    .is_none_or(|expires_at| expires_at > Utc::now())
        })
        .map(|lease| lease.owner_thread_id.to_string())
}

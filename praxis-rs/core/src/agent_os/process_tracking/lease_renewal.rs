use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn renew_command_leases(&self, command: &CommandRecord) {
        let snapshots = {
            let mut state = self.state.write().await;
            command
                .lease_ids
                .iter()
                .filter_map(|lease_id| {
                    let lease = state.leases.get_mut(lease_id)?;
                    lease.expires_at = Some(Utc::now() + AgentOsPolicy::get().lease_ttl());
                    Some(lease.clone())
                })
                .collect::<Vec<_>>()
        };
        for lease in snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
    }
}

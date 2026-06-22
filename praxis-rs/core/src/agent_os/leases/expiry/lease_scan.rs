use super::super::*;

pub(super) struct ExpiredLeaseActions {
    pub(super) cleanup_processes: HashSet<(i32, Option<String>)>,
    pub(super) finish_commands: HashSet<String>,
}

impl AgentOs {
    pub(super) async fn take_expired_leases(
        &self,
        now: chrono::DateTime<Utc>,
    ) -> Vec<ResourceLease> {
        let mut expired = Vec::new();
        let mut state = self.state.write().await;
        let ids: Vec<String> = state
            .leases
            .iter()
            .filter_map(|(lease_id, lease)| {
                lease
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= now)
                    .then(|| lease_id.clone())
            })
            .collect();
        for lease_id in ids {
            if let Some(lease) = state.leases.remove(&lease_id) {
                expired.push(lease);
            }
        }
        expired
    }

    pub(super) async fn record_expired_leases(
        &self,
        expired: Vec<ResourceLease>,
    ) -> ExpiredLeaseActions {
        let mut cleanup_processes = HashSet::new();
        let mut finish_commands = HashSet::new();
        for lease in expired {
            if let Some(process_id) = lease.process_id {
                cleanup_processes.insert((process_id, lease.runtime_owner_id.clone()));
            }
            if let Some(command_id) = lease.command_id.clone() {
                finish_commands.insert(command_id);
            }
            self.record_event(
                "lease_expired",
                Some(lease.owner_thread_id),
                Some(lease.task_id),
                lease.command_id.clone(),
                json!({
                    "lease_id": lease.lease_id,
                    "scope": lease.scope,
                    "process_id": lease.process_id,
                    "runtime_owner_id": lease.runtime_owner_id,
                    "requires_process_cleanup": lease.process_id.is_some(),
                }),
            )
            .await;
        }
        ExpiredLeaseActions {
            cleanup_processes,
            finish_commands,
        }
    }
}

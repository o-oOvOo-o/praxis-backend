mod command_finish;
mod lease_scan;
mod process_cleanup;
mod tickets;

impl super::AgentOs {
    pub(in crate::agent_os) async fn expire_leases(&self) {
        let expired = self.take_expired_leases(chrono::Utc::now()).await;
        let actions = self.record_expired_leases(expired).await;
        self.cleanup_expired_lease_processes(actions.cleanup_processes)
            .await;
        self.finish_expired_lease_commands(actions.finish_commands)
            .await;
    }
}

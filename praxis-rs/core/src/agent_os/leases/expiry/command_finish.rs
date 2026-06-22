use super::super::*;

impl AgentOs {
    pub(super) async fn finish_expired_lease_commands(&self, finish_commands: HashSet<String>) {
        for command_id in finish_commands {
            let _ = self
                .finish_managed_command(
                    command_id.as_str(),
                    Some(-1),
                    b"command terminated because AgentOS lease expired",
                    /*release_leases*/ false,
                )
                .await;
        }
    }
}

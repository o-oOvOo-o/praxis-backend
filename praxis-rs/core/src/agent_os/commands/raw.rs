use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn command_raw_command(
        &self,
        command_id: &str,
    ) -> Option<String> {
        self.state
            .read()
            .await
            .commands
            .get(command_id)
            .map(|command| command.raw_command.clone())
    }
}

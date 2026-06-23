use super::super::*;

impl AgentOs {
    pub(in crate::agent_os::managed_commands::executions::finish) async fn attach_finished_command_artifact(
        &self,
        command_id: &str,
        command: &mut CommandRecord,
        artifact_id: String,
    ) {
        command.artifacts.push(artifact_id);
        let mut state = self.state.write().await;
        state
            .commands
            .insert(command_id.to_string(), command.clone());
    }
}
